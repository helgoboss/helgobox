use crate::application::{
    RealearnControlSurfaceMainTaskSender, SessionCommand, SharedMapping, SharedUnitModel,
    UnitModel, VirtualControlElementType, WeakUnitModel,
};
use crate::base::notification;
use crate::domain::{
    format_as_pretty_hex, ActionInvokedEvent, AdditionalFeedbackEvent, Backbone,
    ChangeInstanceFxArgs, ChangeInstanceTrackArgs, CompartmentKind, ControlSurfaceEventHandler,
    DeviceDiff, EnableInstancesArgs, Exclusivity, FeedbackAudioHookTask, GroupId,
    HelgoboxWindowSnitch, InputDescriptor, InstanceContainerCommonArgs, InstanceFxChangeRequest,
    InstanceId, InstanceTrackChangeRequest, LastTouchedTargetFilter, MainProcessor,
    MessageCaptureEvent, MessageCaptureResult, MidiInDevsConfig, MidiOutDevsConfig, MidiScanResult,
    NormalAudioHookTask, OscDeviceId, OscFeedbackProcessor, OscFeedbackTask, OscScanResult,
    ProcessorContext, QualifiedInstanceEvent, QualifiedMappingId, RealearnAccelerator,
    RealearnAudioHook, RealearnControlSurfaceMainTask, RealearnControlSurfaceMiddleware,
    RealearnTarget, RealearnTargetState, ReaperTarget, ReaperTargetType,
    RequestMidiDeviceIdentityCommand, RequestMidiDeviceIdentityReply, SharedInstance,
    SharedMainProcessors, SharedRealTimeProcessor, Tag, UnitContainer, UnitId,
    UnitOrchestrationEvent, WeakInstance, WeakUnit,
};
use crate::infrastructure::data::{
    CommonCompartmentPresetManager, CompartmentPresetManagerEventHandler, ControllerManager,
    ControllerManagerEventHandler, FileBasedControllerPresetManager, FileBasedMainPresetManager,
    FileBasedPresetLinkManager, LicenseManager, LicenseManagerEventHandler,
    MainPresetSelectionConditions, OscDevice, OscDeviceManager, SharedControllerManager,
    SharedControllerPresetManager, SharedLicenseManager, SharedMainPresetManager,
    SharedOscDeviceManager, SharedPresetLinkManager,
};
use crate::infrastructure::server;
use crate::infrastructure::server::{
    MetricsReporter, RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL,
};
use crate::infrastructure::ui::{menus, MessagePanel};
use base::default_util::is_default;
use base::{
    make_available_globally_in_main_thread_on_demand, panic_util, spawn_in_main_thread, Global,
    NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread,
};

use crate::base::allocator::{RealearnAllocatorIntegration, RealearnDeallocator, GLOBAL_ALLOCATOR};
use crate::base::notification::notify_user_about_anyhow_error;
use crate::infrastructure::plugin::actions::ACTION_DEFS;
use crate::infrastructure::plugin::api_impl::{register_api, unregister_api};
use crate::infrastructure::plugin::debug_util::resolve_symbols_from_clipboard;
use crate::infrastructure::plugin::dynamic_toolbar::add_or_remove_toolbar_button;
use crate::infrastructure::plugin::hidden_helper_panel::HiddenHelperPanel;
use crate::infrastructure::plugin::persistent_toolbar::add_toolbar_button_persistently;
use crate::infrastructure::plugin::tracing_util::TracingHook;
use crate::infrastructure::plugin::{
    ini_util, update_auto_units_async, SharedInstanceShell, WeakInstanceShell,
    ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME,
};
use crate::infrastructure::server::services::Services;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::util::open_child_panel;
use crate::infrastructure::ui::welcome_panel::WelcomePanel;
use anyhow::{bail, Context};
use base::hash_util::NonCryptoHashSet;
use base::metrics_util::MetricsHook;
use helgoboss_allocator::{start_async_deallocation_thread, AsyncDeallocatorCommandReceiver};
use itertools::Itertools;
use once_cell::sync::Lazy;
use realearn_api::persistence::{
    CompartmentPresetId, Controller, ControllerConnection, Envelope, FxChainDescriptor,
    FxDescriptor, MidiControllerConnection, MidiInputPort, MidiOutputPort, TargetTouchCause,
    TrackDescriptor, TrackFxChain,
};
use realearn_api::runtime::{AutoAddedControllerEvent, GlobalInfoEvent};
use reaper_high::{
    ChangeEvent, CrashInfo, Fx, Guid, MiddlewareControlSurface, Project, Reaper, Track,
};
use reaper_low::{PluginContext, Swell};
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{
    reaper_str, AcceleratorPosition, ActionValueChange, CommandId, Hmenu, HookCustomMenu,
    HookPostCommand, HookPostCommand2, MenuHookFlag, MidiInputDeviceId, MidiOutputDeviceId,
    ReaProject, ReaperStr, RegistrationHandle, SectionContext, ToolbarIconMap, WindowContext,
};
use reaper_rx::{ActionRxHookPostCommand, ActionRxHookPostCommand2};
use rxrust::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use slog::{debug, Drain};
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fs, mem};
use strum::IntoEnumIterator;
use swell_ui::{Menu, SharedView, View, ViewManager, Window};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use url::Url;

/// Generates a REAPER-extension-like entry point. It also generates everything that
/// `reaper_vst_plugin!` macro would generate, so we don't need that anymore.
///
/// This needs some explanation: No, we are not a REAPER extension! This extension entry point will
/// not be called by REAPER (because our shared library is located in the "UserPlugins/FX"
/// directory, not in "UserPlugins", so REAPER will treat us as VST plug-in). It will
/// be called by our own Helgobox Extension. And the reason for this is that we want
/// to eagerly initialize certain things already at REAPER start time, without requiring
/// the user to add a VST plug-in instance first. This is the easiest way and doesn't require
/// us to split much logic between extension lib and VST plug-in lib.
///
/// If the Helgobox Extension is not installed, this extension entry point will *not* be called.
/// Things will still work though! The only difference is that things are not initialized eagerly.
/// They will be at the time the first plug-in instance is added.
#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn std::error::Error>> {
    init_backbone_shell(context);
    Ok(())
}

pub fn init_backbone_shell(context: PluginContext) {
    BackboneShell::make_available_globally(|| BackboneShell::init(context));
    BackboneShell::get().show_welcome_screen_if_necessary();
}

/// Queue size for sending feedback tasks to audio hook.
///
/// If we have very many instances, this might not be enough. But the task size is so
/// small, so why not make it a great number? It's global, not per instance. For one
/// instance we had 2000 before and it worked great. With 100_000 we can easily cover 50 instances
/// and yet it's only around 8 MB memory usage (globally). We are on the safe side!
const FEEDBACK_AUDIO_HOOK_TASK_QUEUE_SIZE: usize = 100_000;
/// Queue size for sending less frequent tasks to audio hook.
const NORMAL_AUDIO_HOOK_TASK_QUEUE_SIZE: usize = 2000;
/// Queue size for deferring deallocation in real-time threads to a dedicated deallocator thread.
///
/// - A capacity of 1 means 3 * usize, so 3 * 64 bit = 24 byte.
/// - A capacity of 1000 means around 24 kb then.
/// - So we can easily make this large without using much memory.
/// - Although probably not necessary because deallocation in real-time threads doesn't happen
///   often.
/// - Still, we are on the safe side. Because if the channel is full, it will start deallocating in
///   real-time thread until there's capacity again.
const DEALLOCATOR_THREAD_CAPACITY: usize = 10000;

make_available_globally_in_main_thread_on_demand!(BackboneShell);

static APP_LIBRARY: std::sync::OnceLock<anyhow::Result<crate::infrastructure::ui::AppLibrary>> =
    std::sync::OnceLock::new();

pub type RealearnSessionAccelerator =
    RealearnAccelerator<WeakUnitModel, BackboneHelgoboxWindowSnitch>;

pub type RealearnControlSurface =
    MiddlewareControlSurface<RealearnControlSurfaceMiddleware<WeakUnitModel>>;

/// Just the old term as alias for easier class search.
type _App = BackboneShell;

#[derive(Debug)]
pub struct BackboneShell {
    /// RAII
    _tracing_hook: Option<TracingHook>,
    /// RAII
    _metrics_hook: Option<MetricsHook>,
    state: RefCell<AppState>,
    license_manager: SharedLicenseManager,
    controller_preset_manager: SharedControllerPresetManager,
    main_preset_manager: SharedMainPresetManager,
    preset_link_manager: SharedPresetLinkManager,
    osc_device_manager: SharedOscDeviceManager,
    controller_manager: SharedControllerManager,
    server: SharedRealearnServer,
    config: RefCell<BackboneConfig>,
    sessions_changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    control_surface_main_task_sender: RealearnControlSurfaceMainTaskSender,
    instance_event_sender: SenderToNormalThread<QualifiedInstanceEvent>,
    #[cfg(feature = "playtime")]
    clip_matrix_event_sender: SenderToNormalThread<crate::domain::QualifiedClipMatrixEvent>,
    osc_feedback_task_sender: SenderToNormalThread<OscFeedbackTask>,
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    feedback_audio_hook_task_sender: SenderToRealTimeThread<FeedbackAudioHookTask>,
    instance_orchestration_event_sender: SenderToNormalThread<UnitOrchestrationEvent>,
    audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
    instance_shell_infos: RefCell<Vec<InstanceShellInfo>>,
    unit_infos: RefCell<Vec<UnitInfo>>,
    message_panel: SharedView<MessagePanel>,
    osc_feedback_processor: Rc<RefCell<OscFeedbackProcessor>>,
    proto_hub: crate::infrastructure::proto::ProtoHub,
    welcome_panel: RefCell<Option<SharedView<WelcomePanel>>>,
    /// We need to keep this panel in memory in order to be informed when it's destroyed.
    _shutdown_detection_panel: SharedView<HiddenHelperPanel>,
    audio_block_counter: Arc<AtomicU32>,
}

#[derive(Clone, Debug)]
pub struct InstanceShellInfo {
    pub instance_id: InstanceId,
    pub processor_context: ProcessorContext,
    /// Representation of the instance in the infrastructure layer.
    pub instance_shell: WeakInstanceShell,
    /// Representation of the instance in the domain layer.
    pub instance: WeakInstance,
}

/// Contains all constant info about a Helgobox instance including references to the
/// corresponding instance representations in each onion layer.
#[derive(Debug)]
pub struct UnitInfo {
    pub unit_id: UnitId,
    pub instance_id: InstanceId,
    /// Whether this is the main unit of an instance.
    pub is_main_unit: bool,
    /// Whether this is an automatically loaded unit (according to controller configuration).
    pub is_auto_unit: bool,
    pub processor_context: ProcessorContext,
    /// Representation of the unit's parent instance in the domain layer.
    pub instance: WeakInstance,
    /// User interface of the unit's parent instance in the infrastructure layer.
    pub instance_panel: Weak<InstancePanel>,
    /// Representation of the unit in the application layer.
    pub unit_model: WeakUnitModel,
    /// Representation of the unit in the domain layer.
    pub unit: WeakUnit,
}

#[derive(Debug)]
enum AppState {
    /// As long as no ReaLearn instance is loaded.
    ///
    /// Happens once very shortly at startup and then whenever the last ReaLearn instance
    /// disappears. This can happen multiple times if REAPER preference "Allow complete unload of
    /// VST plug-ins" is disabled (the default).
    Sleeping(SleepingState),
    /// Whenever the first ReaLearn instance is loading.
    WakingUp,
    /// As long as at least one ReaLearn instance is loaded.
    Awake(AwakeState),
    /// Whenever the last ReaLearn instance is unloading.
    GoingToSleep,
    /// Whenever one (not necessarily the last) ReaLearn instance is unloading.
    Suspended,
}

#[derive(Debug)]
struct SleepingState {
    control_surface: Box<RealearnControlSurface>,
    audio_hook: Box<RealearnAudioHook>,
    accelerator: Box<RealearnSessionAccelerator>,
    async_deallocation_receiver: AsyncDeallocatorCommandReceiver,
}

#[derive(Debug)]
struct AwakeState {
    control_surface_handle: RegistrationHandle<RealearnControlSurface>,
    audio_hook_handle: RegistrationHandle<RealearnAudioHook>,
    accelerator_handle: RegistrationHandle<RealearnSessionAccelerator>,
    async_deallocation_thread: JoinHandle<AsyncDeallocatorCommandReceiver>,
    async_runtime: Runtime,
}

impl BackboneShell {
    /// Executed globally just once when module loaded.
    ///
    /// This should fire up everything that must be around even while asleep (even without any
    /// VST plug-in instance being around). The less the better! Users shouldn't pay for stuff they
    /// don't need!
    ///
    /// The opposite function is [Self::dispose].
    pub fn init(context: PluginContext) -> Self {
        let logger = BackboneShell::logger().clone();
        // We need Swell already without VST plug-in instance to populate the extension menu. As soon as an instance
        // exists, we also need it for all the native GUI stuff.
        Swell::make_available_globally(Swell::load(context));
        // We need access to REAPER as soon as possible, of course
        // TODO-medium This needs around 10 MB of RAM. Of course only once, not per instance,
        //  so not a big deal. Still, maybe could be improved?
        Reaper::setup_with_defaults(
            context,
            logger,
            CrashInfo {
                plugin_name: "Helgobox".to_string(),
                plugin_version: BackboneShell::detailed_version_label().to_string(),
                support_email_address: "info@helgoboss.org".to_string(),
            },
        );
        // The API contains functions that must be around without any VST plug-in instance being active
        register_api().expect("couldn't register API");
        // Senders and receivers are initialized here but used only when awake. Yes, they already consume memory
        // when asleep but most of them are unbounded and therefore consume a minimal amount of memory as long as
        // they are not used.
        let config = BackboneConfig::load().unwrap_or_else(|e| {
            debug!(BackboneShell::logger(), "{}", e);
            Default::default()
        });
        let (control_surface_main_task_sender, control_surface_main_task_receiver) =
            SenderToNormalThread::new_unbounded_channel("control surface main tasks");
        let control_surface_main_task_sender =
            RealearnControlSurfaceMainTaskSender(control_surface_main_task_sender);
        #[cfg(feature = "playtime")]
        let (clip_matrix_event_sender, clip_matrix_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("clip matrix events");
        let (osc_feedback_task_sender, osc_feedback_task_receiver) =
            SenderToNormalThread::new_unbounded_channel("osc feedback tasks");
        let (additional_feedback_event_sender, additional_feedback_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("additional feedback events");
        let (instance_orchestration_event_sender, instance_orchestration_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("instance orchestration events");
        let (feedback_audio_hook_task_sender, feedback_audio_hook_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "feedback audio hook tasks",
                FEEDBACK_AUDIO_HOOK_TASK_QUEUE_SIZE,
            );
        let (instance_event_sender, instance_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("instance state change events");
        let (audio_hook_task_sender, normal_audio_hook_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "normal audio hook tasks",
                NORMAL_AUDIO_HOOK_TASK_QUEUE_SIZE,
            );
        // We initialize tracing here already instead of activating/deactivating it when waking up
        // or go to sleep. Reason: It doesn't matter that the async logger thread is lurking around
        // even when no ReaLearn instance exists anymore, because the REALEARN_LOG env variable is
        // opt-in and should only be used for debugging purposes anyway. Also,
        // activating/deactivating would be more difficult because the global tracing subscriber can
        // be set only once. There's no way to unset it.
        let tracing_hook = TracingHook::init();
        // We initialize metrics here already for the same reasons.
        let metrics_hook = MetricsHook::init();
        // The global allocator uses a dedicated thread to offload deallocation from real-time
        // threads. However, this thread will only exist when awake.
        let async_deallocation_receiver = GLOBAL_ALLOCATOR.init(
            DEALLOCATOR_THREAD_CAPACITY,
            RealearnAllocatorIntegration::new(
                Reaper::get()
                    .medium_reaper()
                    .low()
                    .pointers()
                    .IsInRealTimeAudio
                    .expect("couldn't get IsInRealTimeAudio function from REAPER"),
            ),
        );
        // License management
        let license_manager = LicenseManager::new(
            BackboneShell::helgoboss_resource_dir_path().join("licensing.json"),
            Box::new(BackboneLicenseManagerEventHandler),
        );
        // This just initializes the clip engine, it doesn't add any clip matrix yet, so resource consumption is low.
        #[cfg(feature = "playtime")]
        playtime_impl::init_clip_engine(&license_manager);
        // This is the backbone representation in the domain layer, of course we need this now already.
        let backbone_state = Backbone::new(
            additional_feedback_event_sender.clone(),
            RealearnTargetState::new(additional_feedback_event_sender.clone()),
        );
        Backbone::make_available_globally(|| backbone_state);
        // This just sets up the server event wiring but doesn't send any events yet => low resource usage.
        let sessions_changed_subject: RefCell<LocalSubject<'static, (), ()>> = Default::default();
        server::http::keep_informing_clients_about_sessions(
            sessions_changed_subject.borrow().clone(),
        );
        // Presets, preset links and controllers are all global things, so we wire everything now already. However,
        // the actual presets are read from disk when waking up.
        let controller_preset_manager = FileBasedControllerPresetManager::new(
            CompartmentKind::Controller,
            BackboneShell::realearn_compartment_preset_dir_path(CompartmentKind::Controller),
            Box::new(BackboneControllerPresetManagerEventHandler),
        );
        let main_preset_manager = FileBasedMainPresetManager::new(
            CompartmentKind::Main,
            BackboneShell::realearn_compartment_preset_dir_path(CompartmentKind::Main),
            Box::new(BackboneMainPresetManagerEventHandler),
        );
        let preset_link_manager =
            FileBasedPresetLinkManager::new(BackboneShell::realearn_auto_load_configs_dir_path());
        let controller_manager = ControllerManager::new(
            Self::realearn_controller_config_file_path(),
            Box::new(BackboneControllerManagerEventHandler),
        );
        // This doesn't yet load devices or start listening for OSC messages (will happen on wake up)
        let osc_device_manager =
            OscDeviceManager::new(BackboneShell::realearn_osc_device_config_file_path());
        // This doesn't yet start the server (will happen on wake up)
        let server = RealearnServer::new(
            config.main.server_http_port,
            config.main.server_https_port,
            config.main.server_grpc_port,
            BackboneShell::server_resource_dir_path().join("certificates"),
            MetricsReporter::new(),
        );
        // OSC devices are reconnected only if device list changes (= while instance active)
        let osc_feedback_processor = OscFeedbackProcessor::new(osc_feedback_task_receiver);
        osc_device_manager
            .changed()
            .subscribe(|_| BackboneShell::get().reconnect_osc_devices());
        let shared_main_processors = SharedMainProcessors::default();
        // This doesn't yet activate the control surface (will happen on wake up)
        let control_surface = MiddlewareControlSurface::new(RealearnControlSurfaceMiddleware::new(
            BackboneShell::logger(),
            control_surface_main_task_receiver,
            instance_event_receiver,
            #[cfg(feature = "playtime")]
            clip_matrix_event_receiver,
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            shared_main_processors.clone(),
            Box::new(BackboneControlSurfaceEventHandler),
        ));
        // This doesn't yet activate the audio hook (will happen on wake up)
        let audio_block_counter = Arc::new(AtomicU32::new(0));
        let audio_hook = RealearnAudioHook::new(
            normal_audio_hook_task_receiver,
            feedback_audio_hook_task_receiver,
            audio_block_counter.clone(),
        );
        // This doesn't yet activate the accelerator (will happen on wake up)
        let accelerator =
            RealearnAccelerator::new(shared_main_processors, BackboneHelgoboxWindowSnitch);
        // Silently decompress app and load library in background so it's ready when needed. We want to do this
        // already here in order to let actions such as "Show/hide Playtime" work instantly without delay.
        let _ = std::thread::Builder::new()
            .name("Helgobox app loader".to_string())
            .spawn(|| {
                let result = decompress_app().and_then(|_| load_app_library());
                let _ = APP_LIBRARY.set(result);
            });
        // We want actions and menu entries to be available even in sleeping state because there are some convenience
        // actions among them that boot up an instance when none is found yet.
        Self::register_actions();
        let sleeping_state = SleepingState {
            control_surface: Box::new(control_surface),
            audio_hook: Box::new(audio_hook),
            accelerator: Box::new(accelerator),
            async_deallocation_receiver,
        };
        // We wake up reaper-rs here already, otherwise the registered actions wouldn't show up yet.
        Reaper::get().wake_up().expect("couldn't wake up REAPER");
        // Must be called after registering actions and waking REAPER up, otherwise it won't find the command IDs.
        let _ = Self::register_extension_menu();
        let _ = Self::register_toolbar_icon_map();
        for (key, value) in &config.toolbar {
            if *value > 0 {
                let _ = add_or_remove_toolbar_button(key, true);
            }
        }
        // Detect shutdown via hidden child window as suggested by Justin
        let shutdown_detection_panel = SharedView::new(HiddenHelperPanel::new());
        shutdown_detection_panel.clone().open(reaper_window());
        BackboneShell {
            _tracing_hook: tracing_hook,
            _metrics_hook: metrics_hook,
            state: RefCell::new(AppState::Sleeping(sleeping_state)),
            license_manager: Rc::new(RefCell::new(license_manager)),
            controller_preset_manager: Rc::new(RefCell::new(controller_preset_manager)),
            main_preset_manager: Rc::new(RefCell::new(main_preset_manager)),
            preset_link_manager: Rc::new(RefCell::new(preset_link_manager)),
            osc_device_manager: Rc::new(RefCell::new(osc_device_manager)),
            controller_manager: Rc::new(RefCell::new(controller_manager)),
            server: Rc::new(RefCell::new(server)),
            config: RefCell::new(config),
            sessions_changed_subject,
            party_is_over_subject: Default::default(),
            control_surface_main_task_sender,
            instance_event_sender,
            #[cfg(feature = "playtime")]
            clip_matrix_event_sender,
            osc_feedback_task_sender,
            additional_feedback_event_sender,
            feedback_audio_hook_task_sender,
            instance_orchestration_event_sender,
            audio_hook_task_sender,
            instance_shell_infos: RefCell::new(vec![]),
            unit_infos: Default::default(),
            message_panel: Default::default(),
            osc_feedback_processor: Rc::new(RefCell::new(osc_feedback_processor)),
            proto_hub: crate::infrastructure::proto::ProtoHub::new(),
            welcome_panel: Default::default(),
            _shutdown_detection_panel: shutdown_detection_panel,
            audio_block_counter,
        }
    }

    pub fn dispose(&mut self) {
        // This is ugly but we need it on Windows where getting the current thread can lead to
        // "use of std::thread::current() is not possible after the thread's local data has been destroyed"
        // when exiting REAPER. The following code essentially ignores this.
        // See https://github.com/rust-lang/rust/issues/110708
        panic_util::ignore_panics(|| {
            let _ = Reaper::get().go_to_sleep();
            self.message_panel.close();
            self.party_is_over_subject.next(());
            let _ = unregister_api();
        });
    }

    pub fn show_welcome_screen_if_necessary(&self) {
        let showed_already = {
            let mut config = self.config.borrow_mut();
            let value = mem::replace(&mut config.main.showed_welcome_screen, 1);
            config.save().unwrap();
            value == 1
        };
        if !showed_already {
            Self::show_welcome_screen();
        }
    }

    pub fn detailed_version_label() -> &'static str {
        static VALUE: Lazy<String> = Lazy::new(build_detailed_version);
        &VALUE
    }

    pub fn version() -> &'static Version {
        static VALUE: Lazy<Version> = Lazy::new(|| {
            Version::parse(crate::infrastructure::plugin::built_info::PKG_VERSION).unwrap()
        });
        &VALUE
    }

    pub fn create_envelope<T>(value: T) -> Envelope<T> {
        Envelope {
            version: Some(Self::version().clone()),
            value,
        }
    }

    pub fn warn_if_envelope_version_higher(envelope_version: Option<&Version>) {
        if let Some(v) = envelope_version {
            if Self::version() < v {
                notification::warn(format!(
                    "The given snippet was created for ReaLearn {}, which is \
                         newer than the installed version {}. Things might not work as expected. \
                         Please consider upgrading your \
                         ReaLearn installation to the latest version.",
                    v,
                    BackboneShell::version()
                ));
            }
        }
    }

    pub fn get_temp_dir() -> Option<&'static TempDir> {
        static TEMP_DIR: Lazy<Option<TempDir>> =
            Lazy::new(|| tempfile::Builder::new().prefix("realearn-").tempdir().ok());
        TEMP_DIR.as_ref()
    }

    /// This will cause all main processors to "switch all lights off".
    ///
    /// Doing this on main processor drop is too late as audio won't be running anymore and the MIDI devices will
    /// already be closed. Opening them again - in a destructor - is not good practice.
    ///
    /// This should be called early in the REAPER shutdown procedure. At the moment, we call it when a hidden window
    /// is destroyed.
    pub fn shutdown(&self) {
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            let middleware = control_surface.middleware_mut();
            middleware.shutdown();
        });
        // It's important to wait a bit otherwise we risk the MIDI is not being sent.
        // We wait for 3 audio blocks, a maximum of 100 milliseconds. Justin's recommendation.
        let initial_block_count = self.audio_block_count();
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(1));
            let elapsed_blocks = self.audio_block_count().saturating_sub(initial_block_count);
            if elapsed_blocks > 2 {
                tracing::debug!("Waited a total of {elapsed_blocks} blocks after sending shutdown MIDI messages");
                break;
            }
        }
    }

    fn audio_block_count(&self) -> u32 {
        self.audio_block_counter.load(Ordering::Relaxed)
    }

    fn reconnect_osc_devices(&self) {
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            let middleware = control_surface.middleware_mut();
            // Disconnect inputs
            middleware.clear_osc_input_devices();
            // Disconnect outputs
            let mut processor = self.osc_feedback_processor.borrow_mut();
            processor.stop();
            // Reconnect inputs and outputs
            let (osc_input_devices, osc_output_devices) = self
                .osc_device_manager
                .borrow_mut()
                .connect_all_enabled_inputs_and_outputs();
            middleware.set_osc_input_devices(osc_input_devices);
            processor.start(osc_output_devices);
        });
    }

    fn create_services(&self) -> Services {
        Services {
            helgobox_service: server::services::helgobox_service::create_server(&self.proto_hub),
        }
    }

    /// Executed whenever the first Helgobox instance is loaded.
    ///
    /// This should fire up stuff that only needs to be around while awake (= as long as at least one Helgobox VST
    /// plug-in instance is around). Stuff that must be around even while asleep should be put into [Self::init].
    ///
    /// The opposite function is [Self::go_to_sleep].
    pub fn wake_up(&self) {
        let prev_state = self.state.replace(AppState::WakingUp);
        let AppState::Sleeping(mut sleeping_state) = prev_state else {
            panic!("App was not sleeping");
        };
        // (Re)load presets, links and controllers
        let _ = self
            .controller_preset_manager
            .borrow_mut()
            .load_presets_from_disk_without_notification();
        let _ = self
            .main_preset_manager
            .borrow_mut()
            .load_presets_from_disk_without_notification();
        let _ = self
            .preset_link_manager
            .borrow_mut()
            .load_preset_links_from_disk();
        let _ = self
            .controller_manager
            .borrow_mut()
            .load_controllers_from_disk();
        let _ = self
            .osc_device_manager
            .borrow_mut()
            .load_osc_devices_from_disk();
        // Start thread for async deallocation
        let async_deallocation_thread = start_async_deallocation_thread(
            RealearnDeallocator::with_metrics("helgobox.allocator.async_deallocation"),
            sleeping_state.async_deallocation_receiver,
        );
        // Start async runtime
        let async_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("Helgobox async runtime")
            .worker_threads(1)
            .build()
            .expect("couldn't start ReaLearn async runtime");
        // Activate server
        if self.config.borrow().server_is_enabled() {
            self.server()
                .borrow_mut()
                .start(&async_runtime, self.create_services())
                .unwrap_or_else(warn_about_failed_server_start);
        }
        let mut session = Reaper::get().medium_session();
        // Action hooks
        session
            .plugin_register_add_hook_post_command::<ActionRxHookPostCommand<Global>>()
            .unwrap();
        session
            .plugin_register_add_hook_post_command::<Self>()
            .unwrap();
        // This fails before REAPER 6.20 and therefore we don't have MIDI CC action feedback.
        let _ =
            session.plugin_register_add_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        let _ = session.plugin_register_add_hook_post_command_2::<Self>();
        // Audio hook
        debug!(
            BackboneShell::logger(),
            "Registering ReaLearn audio hook and control surface..."
        );
        let audio_hook_handle = session
            .audio_reg_hardware_hook_add(sleeping_state.audio_hook)
            .expect("couldn't register ReaLearn audio hook");
        // OSC devices
        let (osc_input_devices, osc_output_devices) = self
            .osc_device_manager
            .borrow_mut()
            .connect_all_enabled_inputs_and_outputs();
        // OSC processor
        self.osc_feedback_processor
            .borrow_mut()
            .start(osc_output_devices);
        // Control surface
        let middleware = sleeping_state.control_surface.middleware_mut();
        middleware.set_osc_input_devices(osc_input_devices);
        sleeping_state.control_surface.middleware().wake_up();
        let control_surface_handle = session
            .plugin_register_add_csurf_inst(sleeping_state.control_surface)
            .expect("couldn't register ReaLearn control surface");
        // Accelerator
        let accelerator_handle = session
            .plugin_register_add_accelerator_register(
                sleeping_state.accelerator,
                AcceleratorPosition::Front,
            )
            .expect("couldn't register ReaLearn accelerator");
        // Awake state
        let awake_state = AwakeState {
            control_surface_handle,
            audio_hook_handle,
            accelerator_handle,
            async_deallocation_thread,
            async_runtime,
        };
        self.state.replace(AppState::Awake(awake_state));
    }

    // Executed whenever the last ReaLearn instance goes away.
    pub fn go_to_sleep(&self) {
        let prev_state = self.state.replace(AppState::GoingToSleep);
        let awake_state = if let AppState::Awake(s) = prev_state {
            s
        } else {
            panic!("App was not awake when trying to go to sleep");
        };
        let mut session = Reaper::get().medium_session();
        debug!(
            BackboneShell::logger(),
            "Unregistering ReaLearn control surface and audio hook..."
        );
        let (accelerator, mut control_surface, audio_hook) = unsafe {
            let accelerator = session
                .plugin_register_remove_accelerator(awake_state.accelerator_handle)
                .expect("accelerator was not registered");
            let control_surface = session
                .plugin_register_remove_csurf_inst(awake_state.control_surface_handle)
                .expect("control surface was not registered");
            let audio_hook = session
                .audio_reg_hardware_hook_remove(awake_state.audio_hook_handle)
                .expect("control surface was not registered");
            (accelerator, control_surface, audio_hook)
        };
        // Close OSC connections
        let middleware = control_surface.middleware_mut();
        middleware.clear_osc_input_devices();
        self.osc_feedback_processor.borrow_mut().stop();
        // Actions
        session.plugin_register_remove_hook_post_command_2::<Self>();
        session.plugin_register_remove_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        session.plugin_register_remove_hook_post_command::<Self>();
        session.plugin_register_remove_hook_post_command::<ActionRxHookPostCommand<Global>>();
        // Server
        self.server().borrow_mut().stop();
        // Shutdown async runtime
        tracing::info!("Shutting down async runtime...");
        awake_state
            .async_runtime
            .shutdown_timeout(Duration::from_secs(1));
        tracing::info!("Async runtime shut down successfully");
        // Stop async deallocation thread
        GLOBAL_ALLOCATOR.stop_async_deallocation();
        let async_deallocation_receiver = awake_state
            .async_deallocation_thread
            .join()
            .expect("couldn't join deallocation thread");
        // Finally go to sleep
        let sleeping_state = SleepingState {
            control_surface,
            audio_hook,
            accelerator,
            async_deallocation_receiver,
        };
        self.state.replace(AppState::Sleeping(sleeping_state));
    }

    /// Attention: The real-time processor is removed *async*! That means it can still be called
    /// by the audio hook, even after this method has executed. The benefit is that it will still
    /// be able to do clean-up work after the plug-in instance is gone as long as another one is
    /// still around. The problem is that the real-time processor has a reference to the plug-in
    /// host callback and is *must not* call it when the plug-in is already gone, otherwise boom!
    /// Unfortunately there's no easy way to detect if it's gone or not.
    ///
    /// What options do we have (we decided for option 7)?
    ///
    /// 1. Setting the callback to None async by sending a message via channel? No!
    ///     - This would affect the next audio thread cycle only. But the audio hook first runs
    ///       real-time processors and then tasks.
    ///     - And even if we would swap the order (which would defeat the purpose), this would still
    ///       leave the possibility that the real-time processor will be executed before the task is
    ///       processed.
    /// 2. Setting the callback to None by calling the real-time processor directly? No!
    ///     - The real-time processor must only be invoked from the audio thread. We want to avoid
    ///       wrapping the processor with a mutex because in all other cases we don't need
    ///       synchronized access (we work with channels instead).
    ///     - Also, a similar issue as in point 1 could arise: The real-time processor could just be
    ///       in the middle of its run call! We would need the mutex to wait for any current call to
    ///       end.
    /// 3. Unregister audio hook, remove real-time processor and register it again? Maybe.
    ///     - First, this would only work if REAPER ensures that it waits until the current call of
    ///       this hook is finished before returning from the removal function. Justin confirmed
    ///       that it works that way, yay!
    ///     - We would lose the possibility of doing clean-up work though.
    /// 4. Wrapping the host callback as `Arc<Mutex<Option<HostCallback>>>`? Okay.
    ///     - Then the synchronized access would be concentrated on this tiny necessity only, which
    ///       would be okay.
    ///     - Looks like a lot of fuzz though.
    /// 5. Use `Arc<HostCallback>` and `Weak<HostCallback>` and always check if upgrade fails? Okay.
    ///     - Nice! But how about performance if we always have to inc/dec the `AtomicUsize`?
    /// 6. Use `HostCallback#is_effect_valid()` magic number logic as "gone" check? Interesting.
    ///     - Could be better than 4 and 5 performance-wise.
    ///     - But what if the audio thread is just in the middle of the host callback call while the
    ///       plug-in is being unloaded? Probably unlikely in practice and maybe not harmful if it
    ///       happens. But still leaves a bad aftertaste. I mean, if we would call the host callback
    ///       from the plug-in processing method only (like normal people do), the host would
    ///       probably take care that this never happens...? This also concerns 4 and 5 by the way.
    /// 7. Just don't use the host callback when called from audio hook? **Yes!**.
    ///     - We use the host callback for checking play state and sending MIDI to FX output.
    ///     - Checking play state could be avoided completely by using REAPER functions.
    ///     - Sending MIDI to FX output is a bit strange anyway if called by the audio hook, maybe
    ///       even something one shouldn't do? It's something global driving something which is
    ///       built to be local only.
    ///     - **Most importantly:** Sending MIDI to FX output while VST processing is stopped will
    ///       not have any effect anyway because the rest of the FX chain is stopped!
    ///     - Solution: Drive the real-time processor from both plug-in `process()` method **and**
    ///       audio hook and make sure that only the call from the plug-in ever sends MIDI to FX
    ///       output.
    fn unregister_unit_real_time_processor(&self, unit_id: UnitId) {
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::RemoveRealTimeProcessor(unit_id));
    }

    /// We remove the main processor synchronously because it allows us to keep its fail-fast
    /// behavior. E.g. we can still panic if DomainEventHandler (weak session) or channel
    /// receivers are gone because we know it's not supposed to happen. Also, unlike with
    /// real-time processor, whatever cleanup work is necessary, we can do right here because we
    /// are in main thread already.
    fn unregister_unit_main_processor(&self, unit_id: UnitId) {
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            // Remove main processor.
            control_surface
                .middleware_mut()
                .remove_main_processor(unit_id);
        });
    }

    pub fn feedback_audio_hook_task_sender(
        &self,
    ) -> &SenderToRealTimeThread<FeedbackAudioHookTask> {
        &self.feedback_audio_hook_task_sender
    }

    pub fn instance_event_sender(&self) -> &SenderToNormalThread<QualifiedInstanceEvent> {
        &self.instance_event_sender
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_event_sender(
        &self,
    ) -> &SenderToNormalThread<crate::domain::QualifiedClipMatrixEvent> {
        &self.clip_matrix_event_sender
    }

    pub fn normal_audio_hook_task_sender(&self) -> &SenderToRealTimeThread<NormalAudioHookTask> {
        &self.audio_hook_task_sender
    }

    pub fn additional_feedback_event_sender(
        &self,
    ) -> SenderToNormalThread<AdditionalFeedbackEvent> {
        self.additional_feedback_event_sender.clone()
    }

    pub fn instance_orchestration_event_sender(
        &self,
    ) -> SenderToNormalThread<UnitOrchestrationEvent> {
        self.instance_orchestration_event_sender.clone()
    }

    pub fn osc_feedback_task_sender(&self) -> &SenderToNormalThread<OscFeedbackTask> {
        &self.osc_feedback_task_sender
    }

    pub fn control_surface_main_task_sender(&self) -> &RealearnControlSurfaceMainTaskSender {
        &self.control_surface_main_task_sender
    }

    pub fn proto_hub(&self) -> &crate::infrastructure::proto::ProtoHub {
        &self.proto_hub
    }

    fn temporarily_reclaim_control_surface_ownership(
        &self,
        f: impl FnOnce(&mut RealearnControlSurface),
    ) {
        let next_state = match self.state.replace(AppState::Suspended) {
            AppState::Sleeping(mut s) => {
                f(&mut s.control_surface);
                AppState::Sleeping(s)
            }
            AppState::Awake(s) => {
                let mut session = Reaper::get().medium_session();
                let mut control_surface = unsafe {
                    session
                        .plugin_register_remove_csurf_inst(s.control_surface_handle)
                        .expect("control surface was not registered")
                };
                // Execute necessary operations
                f(&mut control_surface);
                // Give it back to REAPER.
                let control_surface_handle = session
                    .plugin_register_add_csurf_inst(control_surface)
                    .expect("couldn't reregister ReaLearn control surface");
                let awake_state = AwakeState {
                    control_surface_handle,
                    audio_hook_handle: s.audio_hook_handle,
                    accelerator_handle: s.accelerator_handle,
                    async_deallocation_thread: s.async_deallocation_thread,
                    async_runtime: s.async_runtime,
                };
                AppState::Awake(awake_state)
            }
            _ => panic!("Backbone was neither in sleeping nor in awake state"),
        };
        self.state.replace(next_state);
    }

    /// Spawns the given future on the ReaLearn async runtime.
    ///
    /// # Panics
    ///
    /// Panics if called in any state other than awake.
    #[allow(dead_code)]
    pub fn spawn_in_async_runtime<R>(
        &self,
        f: impl Future<Output = R> + Send + 'static,
    ) -> tokio::task::JoinHandle<R>
    where
        R: Send + 'static,
    {
        self.with_async_runtime(|runtime| runtime.spawn(f))
            .expect("couldn't use runtime")
    }

    fn with_async_runtime<R>(&self, f: impl FnOnce(&Runtime) -> R) -> anyhow::Result<R> {
        let state = self.state.borrow();
        let AppState::Awake(state) = &*state else {
            bail!("attempt to access async runtime while ReaLearn in wrong state: {state:?}");
        };
        Ok(f(&state.async_runtime))
    }

    pub fn license_manager(&self) -> &SharedLicenseManager {
        &self.license_manager
    }

    pub fn controller_preset_manager(&self) -> &SharedControllerPresetManager {
        &self.controller_preset_manager
    }

    pub fn main_preset_manager(&self) -> &SharedMainPresetManager {
        &self.main_preset_manager
    }

    pub fn controller_manager(&self) -> &SharedControllerManager {
        &self.controller_manager
    }

    pub fn compartment_preset_manager(
        &self,
        compartment: CompartmentKind,
    ) -> Rc<RefCell<dyn CommonCompartmentPresetManager>> {
        match compartment {
            CompartmentKind::Controller => self.controller_preset_manager().clone(),
            CompartmentKind::Main => self.main_preset_manager().clone(),
        }
    }

    pub fn preset_link_manager(&self) -> SharedPresetLinkManager {
        self.preset_link_manager.clone()
    }

    pub fn osc_device_manager(&self) -> SharedOscDeviceManager {
        self.osc_device_manager.clone()
    }

    pub fn do_with_osc_device(&self, dev_id: OscDeviceId, f: impl FnOnce(&mut OscDevice)) {
        let mut dev = BackboneShell::get()
            .osc_device_manager()
            .borrow()
            .find_device_by_id(&dev_id)
            .unwrap()
            .clone();
        f(&mut dev);
        BackboneShell::get()
            .osc_device_manager()
            .borrow_mut()
            .update_device(dev)
            .unwrap();
    }

    pub fn server(&self) -> &SharedRealearnServer {
        &self.server
    }

    pub fn config(&self) -> Ref<BackboneConfig> {
        self.config.borrow()
    }

    pub fn server_is_running(&self) -> bool {
        self.server.borrow().is_running()
    }

    pub fn start_server_persistently(&self) -> Result<(), String> {
        let start_result = self
            .with_async_runtime(|runtime| {
                self.server
                    .borrow_mut()
                    .start(runtime, self.create_services())
            })
            .map_err(|e| e.to_string())?;
        self.change_config(BackboneConfig::enable_server);
        start_result
    }

    pub fn stop_server_persistently(&self) {
        self.change_config(BackboneConfig::disable_server);
        self.server.borrow_mut().stop();
    }

    /// Requires REAPER version >= 711+dev0305.
    pub fn toggle_toolbar_button_dynamically(&self, command_name: &str) -> anyhow::Result<()> {
        self.change_config(|config| {
            // Adjust config
            let value = config.toolbar.entry(command_name.to_string()).or_insert(0);
            let enable = *value == 0;
            *value = enable.into();
            // Apply
            add_or_remove_toolbar_button(command_name, enable)?;
            Ok(())
        })
    }

    /// Logging debug info is always initiated by a particular session.
    pub fn log_debug_info(&self, session_id: &str) {
        let msg = format!(
            "\n\
        # App\n\
        \n\
        - State: {:#?}\n\
        - Session count: {}\n\
        - Module base address: {:?}\n\
        ",
            self.state.borrow(),
            self.unit_infos.borrow().len(),
            determine_module_base_address().map(|addr| format!("0x{addr:x}")),
        );
        Reaper::get().show_console_msg(msg);
        self.server.borrow().log_debug_info(session_id);
        self.controller_preset_manager.borrow().log_debug_info();
    }

    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.sessions_changed_subject.borrow().clone()
    }

    fn change_config<R>(&self, op: impl FnOnce(&mut BackboneConfig) -> R) -> R {
        let mut config = self.config.borrow_mut();
        let result = op(&mut config);
        config.save().unwrap();
        self.notify_changed();
        result
    }

    fn helgoboss_resource_dir_path() -> PathBuf {
        Reaper::get().resource_path().join("Helgoboss")
    }

    pub fn app_dir_path() -> PathBuf {
        BackboneShell::helgoboss_resource_dir_path().join("App")
    }

    pub fn app_binary_base_dir_path() -> PathBuf {
        BackboneShell::app_dir_path().join("bin")
    }

    pub fn app_config_dir_path() -> PathBuf {
        BackboneShell::app_dir_path().join("etc")
    }

    pub fn app_settings_file_path() -> PathBuf {
        BackboneShell::app_config_dir_path().join("settings.json")
    }

    pub fn read_app_settings() -> Option<String> {
        fs::read_to_string(Self::app_settings_file_path()).ok()
    }

    pub fn write_app_settings(settings: String) -> anyhow::Result<()> {
        let file_path = Self::app_settings_file_path();
        fs::create_dir_all(
            file_path
                .parent()
                .context("app settings file should have parent")?,
        )?;
        fs::write(file_path, settings)?;
        Ok(())
    }

    fn realearn_resource_dir_path() -> PathBuf {
        BackboneShell::helgoboss_resource_dir_path().join("ReaLearn")
    }

    pub fn realearn_data_dir_path() -> PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/realearn")
    }

    pub fn app_archive_file_path() -> PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/archives/helgobox-app.tar.zst")
    }

    pub fn realearn_high_click_sound_path() -> Option<&'static Path> {
        static PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
            // Before including the audio file in the binary, there was an actual file distributed
            // via ReaPack. However, we had to copy it to a temporary directory anyway, otherwise
            // we would risk an error on Windows when attempting to install a new ReaLearn version
            // via ReaPack while still having ReaLearn open.
            // https://github.com/helgoboss/realearn/issues/780
            // Encoding the file in the binary frees us from having to distribute it.
            let bytes = include_bytes!("../../../../resources/sounds/click-high.mp3");
            let dest_path = BackboneShell::get_temp_dir()?.path().join("click-high.mp3");
            fs::write(&dest_path, bytes).ok()?;
            Some(dest_path)
        });
        PATH.as_ref().map(|p| p.as_path())
    }

    #[cfg(feature = "egui")]
    pub fn realearn_pot_preview_template_path() -> Option<&'static Path> {
        static PATH: Lazy<Option<PathBuf>> = Lazy::new(|| {
            let bytes = include_bytes!(
                "../../../../resources/template-projects/pot-preview/pot-preview.RPP"
            );
            let dest_path = BackboneShell::get_temp_dir()?
                .path()
                .join("pot-preview.RPP");
            fs::write(&dest_path, bytes).ok()?;
            Some(dest_path)
        });
        PATH.as_ref().map(|p| p.as_path())
    }

    pub fn realearn_preset_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("presets")
    }

    pub fn realearn_compartment_preset_dir_path(compartment: CompartmentKind) -> PathBuf {
        let sub_dir = match compartment {
            CompartmentKind::Controller => "controller",
            CompartmentKind::Main => "main",
        };
        Self::realearn_preset_dir_path().join(sub_dir)
    }

    pub fn realearn_auto_load_configs_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("auto-load-configs")
    }

    pub fn realearn_osc_device_config_file_path() -> PathBuf {
        BackboneShell::realearn_resource_dir_path().join("osc.json")
    }

    pub fn realearn_controller_config_file_path() -> PathBuf {
        BackboneShell::realearn_resource_dir_path().join("controllers.json")
    }

    // We need this to be static because we need it at plugin construction time, so we don't have
    // REAPER API access yet. App needs REAPER API to be constructed (e.g. in order to
    // know where's the resource directory that contains the app configuration).
    // TODO-low In future it might be wise to turn to a different logger as soon as REAPER API
    //  available. Then we can also do file logging to ReaLearn resource folder.
    pub fn logger() -> &'static slog::Logger {
        static APP_LOGGER: once_cell::sync::Lazy<slog::Logger> = once_cell::sync::Lazy::new(|| {
            env_logger::init_from_env("REALEARN_LOG");
            slog::Logger::root(slog_stdlog::StdLog.fuse(), slog::o!("app" => "ReaLearn"))
        });
        &APP_LOGGER
    }

    pub fn get_app_library() -> anyhow::Result<&'static crate::infrastructure::ui::AppLibrary> {
        let app_library = APP_LIBRARY
            .get()
            .context("App not loaded yet. Please try again later.")?
            .as_ref();
        app_library.map_err(|e| anyhow::anyhow!(format!("{e:?}")))
    }

    pub fn has_unit_with_key(&self, unit_key: &str) -> bool {
        self.find_unit_model_by_key(unit_key).is_some()
    }

    pub fn find_unit_model_by_key(&self, session_id: &str) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            let Ok(session) = session.try_borrow() else {
                return false;
            };
            session.unit_key() == session_id
        })
    }

    pub fn get_instance_shell_by_instance_id(
        &self,
        instance_id: InstanceId,
    ) -> anyhow::Result<SharedInstanceShell> {
        self.find_instance_shell_by_instance_id(instance_id)
            .context("couldn't find instance")
    }

    pub fn find_instance_shell_by_instance_id(
        &self,
        instance_id: InstanceId,
    ) -> Option<SharedInstanceShell> {
        self.instance_shell_infos
            .borrow()
            .iter()
            .find(|info| info.instance_id == instance_id)
            .and_then(|info| info.instance_shell.upgrade())
    }

    #[allow(dead_code)]
    pub fn find_instance_by_instance_id(&self, instance_id: InstanceId) -> Option<SharedInstance> {
        self.find_main_unit_info_by_instance_id(instance_id)
            .and_then(|i| i.instance.upgrade())
    }

    #[allow(dead_code)]
    pub fn find_instance_panel_by_instance_id(
        &self,
        instance_id: InstanceId,
    ) -> Option<SharedView<InstancePanel>> {
        self.find_main_unit_info_by_instance_id(instance_id)
            .and_then(|i| i.instance_panel.upgrade())
    }

    fn find_main_unit_info_by_instance_id(&self, instance_id: InstanceId) -> Option<Ref<UnitInfo>> {
        let units = self.unit_infos.borrow();
        Ref::filter_map(units, |units| {
            units
                .iter()
                .find(|i| i.is_main_unit && i.instance_id == instance_id)
        })
        .ok()
    }

    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix<R>(
        &self,
        clip_matrix_id: InstanceId,
        f: impl FnOnce(&playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let instance = self
            .find_instance_by_instance_id(clip_matrix_id)
            .context("instance not found")?;
        Backbone::get().with_clip_matrix(&instance, f)
    }

    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix_mut<R>(
        &self,
        clip_matrix_id: InstanceId,
        f: impl FnOnce(&mut playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let instance = self
            .find_instance_by_instance_id(clip_matrix_id)
            .context("instance not found")?;
        Backbone::get().with_clip_matrix_mut(&instance, f)
    }

    #[allow(unused)]
    pub fn create_clip_matrix(&self, clip_matrix_id: InstanceId) -> anyhow::Result<()> {
        let instance_shell = self
            .find_instance_shell_by_instance_id(clip_matrix_id)
            .context("instance not found")?;
        instance_shell.insert_owned_clip_matrix_if_necessary()?;
        Ok(())
    }

    pub fn find_session_by_id_ignoring_borrowed_ones(
        &self,
        session_id: &str,
    ) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            if let Ok(session) = session.try_borrow() {
                session.unit_key() == session_id
            } else {
                false
            }
        })
    }

    pub fn find_session_id_by_instance_id(&self, instance_id: UnitId) -> Option<String> {
        let session = self.find_unit_model_by_unit_id_ignoring_borrowed_ones(instance_id)?;
        let session = session.borrow();
        Some(session.unit_key().to_string())
    }

    pub fn find_unit_id_by_unit_key(&self, session_id: &str) -> Option<UnitId> {
        let session = self.find_unit_model_by_key(session_id)?;
        let session = session.borrow();
        Some(session.unit_id())
    }

    pub fn find_unit_model_by_unit_id_ignoring_borrowed_ones(
        &self,
        instance_id: UnitId,
    ) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            if let Ok(session) = session.try_borrow() {
                session.unit_id() == instance_id
            } else {
                false
            }
        })
    }

    fn find_original_mapping(
        &self,
        initiator_instance_id: UnitId,
        id: QualifiedMappingId,
    ) -> Result<SharedMapping, &'static str> {
        let session = self
            .find_unit_model_by_unit_id_ignoring_borrowed_ones(initiator_instance_id)
            .ok_or("initiator session not found")?;
        let session = session.borrow();
        let mapping = session
            .find_mapping_by_id(id.compartment, id.id)
            .ok_or("origin mapping not found")?;
        Ok(mapping.clone())
    }

    pub fn find_session(
        &self,
        predicate: impl FnMut(&SharedUnitModel) -> bool,
    ) -> Option<SharedUnitModel> {
        self.unit_infos
            .borrow()
            .iter()
            .filter_map(|s| s.unit_model.upgrade())
            .find(predicate)
    }

    pub fn find_first_helgobox_instance_matching(
        &self,
        meets_criteria: impl Fn(&InstanceShellInfo) -> bool,
    ) -> Option<InstanceId> {
        self.instance_shell_infos
            .borrow()
            .iter()
            .filter_map(|info| {
                if !meets_criteria(info) {
                    return None;
                }
                let track_index = info.processor_context.track()?.index()?;
                Some((track_index, info))
            })
            .sorted_by_key(|(track_index, _)| *track_index)
            .next()
            .map(|(_, instance)| instance.instance_id)
    }

    pub fn instance_count(&self) -> usize {
        self.instance_shell_infos.borrow().len()
    }

    pub fn with_instance_shell_infos<R>(&self, f: impl FnOnce(&[InstanceShellInfo]) -> R) -> R {
        f(&self.instance_shell_infos.borrow())
    }

    pub fn with_unit_infos<R>(&self, f: impl FnOnce(&[UnitInfo]) -> R) -> R {
        f(&self.unit_infos.borrow())
    }

    pub fn find_session_by_containing_fx(&self, fx: &Fx) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().containing_fx() == fx
        })
    }

    pub fn register_instance(&self, instance_shell: &SharedInstanceShell) {
        debug!(Reaper::get().logger(), "Registering new instance...");
        let instance = Rc::downgrade(instance_shell.instance());
        let instance_id = instance_shell.instance_id();
        let rt_instance = instance_shell.rt_instance();
        let info = InstanceShellInfo {
            instance_id,
            processor_context: instance_shell.processor_context(),
            instance_shell: Arc::downgrade(instance_shell),
            instance: instance.clone(),
        };
        self.instance_shell_infos.borrow_mut().push(info);
        let instance = Rc::downgrade(instance_shell.instance());
        Backbone::get().register_instance(instance_id, instance.clone());
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::AddRealTimeInstance(
                instance_id,
                rt_instance,
            ));
        self.control_surface_main_task_sender.0.send_complaining(
            RealearnControlSurfaceMainTask::AddInstance(instance_id, instance),
        );
    }

    pub fn unregister_instance(&self, instance_id: InstanceId) {
        debug!(Reaper::get().logger(), "Unregistering instance...");
        self.instance_shell_infos
            .borrow_mut()
            .retain(|i| i.instance_id != instance_id);
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            // Remove main processor.
            control_surface
                .middleware_mut()
                .remove_instance(instance_id);
        });
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::RemoveRealTimeInstance(instance_id));
    }

    pub fn register_unit(
        &self,
        unit_info: UnitInfo,
        real_time_processor: SharedRealTimeProcessor,
        main_processor: MainProcessor<WeakUnitModel>,
    ) {
        let unit_id = unit_info.unit_id;
        debug!(Reaper::get().logger(), "Registering new unit {unit_id}...");
        let mut units = self.unit_infos.borrow_mut();
        if !unit_info.is_auto_unit {
            update_auto_units_async();
        }
        units.push(unit_info);
        debug!(
            Reaper::get().logger(),
            "Unit {unit_id} registered. Unit count: {}",
            units.len()
        );
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::AddRealTimeProcessor(
                unit_id,
                real_time_processor,
            ));
        self.control_surface_main_task_sender.0.send_complaining(
            RealearnControlSurfaceMainTask::AddMainProcessor(main_processor),
        );
    }

    pub fn unregister_unit(&self, unit_id: UnitId) {
        self.unregister_unit_main_processor(unit_id);
        self.unregister_unit_real_time_processor(unit_id);
        debug!(Reaper::get().logger(), "Unregistering unit...");
        let mut units = self.unit_infos.borrow_mut();
        units.retain(|i| {
            if i.unit_id != unit_id {
                // Keep unit
                return true;
            }
            // Remove that unit
            if !i.is_auto_unit {
                update_auto_units_async();
            }
            false
        });
        debug!(
            Reaper::get().logger(),
            "Unit unregistered. Remaining count of units: {}",
            units.len()
        );
    }

    pub fn show_message_panel(
        &self,
        title: impl Into<String>,
        msg: impl Into<String>,
        on_close: impl FnOnce() + 'static,
    ) {
        self.message_panel
            .set_content(title.into(), msg.into(), on_close);
        if !self.message_panel.is_open() {
            self.message_panel.clone().open_without_parent();
        }
    }

    pub fn close_message_panel(&self) {
        self.message_panel.close();
    }

    fn register_extension_menu() -> anyhow::Result<()> {
        let reaper = Reaper::get();
        reaper.medium_reaper().add_extensions_main_menu();
        reaper
            .medium_session()
            .plugin_register_add_hook_custom_menu::<Self>()?;
        Ok(())
    }

    fn register_toolbar_icon_map() -> anyhow::Result<()> {
        Reaper::get()
            .medium_session()
            .plugin_register_add_toolbar_icon_map::<Self>()?;
        Ok(())
    }

    fn register_actions() {
        for def in ACTION_DEFS {
            def.register();
        }
    }

    /// This is only necessary for REAPER versions < 7.11+dev0305
    pub fn add_toolbar_buttons_persistently() {
        for def in ACTION_DEFS.iter().filter(|def| def.add_toolbar_button) {
            let result = add_toolbar_button_persistently(
                def.command_name,
                &def.build_full_action_name(),
                def.icon_file_name,
            );
            if let Err(e) = result {
                notify_user_about_anyhow_error(e);
                return;
            }
        }
        // alert("Successfully added or updated Helgobox toolbar buttons. Please restart REAPER to see them!");
    }

    pub fn show_welcome_screen() {
        let shell = Self::get();
        open_child_panel(&shell.welcome_panel, WelcomePanel::new(), reaper_window());
    }

    pub fn resolve_symbols_from_clipboard() {
        if let Err(e) = resolve_symbols_from_clipboard() {
            Reaper::get().show_console_msg(format!("{e}\n"));
        }
    }

    pub fn find_first_mapping_by_target() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .find_first_mapping_by_target_async(CompartmentKind::Main)
                .await;
            Ok(())
        });
    }

    pub fn open_first_pot_browser() {
        let Some(session) = BackboneShell::get().find_first_relevant_session_monitoring_first()
        else {
            return;
        };
        session.borrow().ui().show_pot_browser();
    }

    pub fn find_first_mapping_by_source() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .find_first_mapping_by_source_async(CompartmentKind::Main)
                .await;
            Ok(())
        });
    }

    pub fn learn_mapping_reassigning_source_open() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .learn_mapping_reassigning_source_async(CompartmentKind::Main, true)
                .await;
            Ok(())
        });
    }

    pub fn send_feedback_for_all_instances() {
        Self::get()
            .control_surface_main_task_sender
            .0
            .send_complaining(RealearnControlSurfaceMainTask::SendAllFeedback);
    }

    pub fn learn_mapping_reassigning_source() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .learn_mapping_reassigning_source_async(CompartmentKind::Main, false)
                .await;
            Ok(())
        });
    }

    pub fn learn_source_for_last_touched_target() {
        let included_target_types = ReaperTargetType::iter().collect();
        let filter = LastTouchedTargetFilter {
            included_target_types: &included_target_types,
            touch_cause: TargetTouchCause::Any,
        };
        let target = Backbone::get().find_last_touched_target(filter);
        let target = match target.as_ref() {
            None => return,
            Some(t) => t,
        };
        BackboneShell::get().start_learning_source_for_target(CompartmentKind::Main, target);
    }

    pub fn toggle_app_focus() {
        let _ = Self::get().toggle_app_focus_internal();
    }

    fn toggle_app_focus_internal(&self) -> anyhow::Result<()> {
        let project = Reaper::get().current_project();
        let instance_id = self
            .find_first_helgobox_instance_in_project(project)
            .context("no Helgobox instance")?;
        let instance_panel = self
            .find_instance_panel_by_instance_id(instance_id)
            .context("instance panel not found")?;
        instance_panel.toggle_app_instance_focus();
        Ok(())
    }

    pub fn find_first_helgobox_instance_in_project(&self, project: Project) -> Option<InstanceId> {
        self.find_first_helgobox_instance_matching(|instance| {
            instance
                .processor_context
                .project()
                .is_some_and(|p| p == project)
        })
    }

    pub fn find_first_playtime_helgobox_instance_in_project(
        &self,
        project: Project,
    ) -> Option<InstanceId> {
        self.find_first_helgobox_instance_matching(|info| {
            if info.processor_context.project() != Some(project) {
                return false;
            }
            let Some(instance) = info.instance.upgrade() else {
                return false;
            };
            let instance_state = instance.borrow();
            instance_state.has_clip_matrix()
        })
    }

    pub fn show_hide_playtime() {
        #[cfg(feature = "playtime")]
        {
            playtime_impl::show_or_hide_playtime().expect("couldn't show/hide playtime");
        }
    }

    async fn find_first_mapping_by_source_async(
        &self,
        compartment: CompartmentKind,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some control elements!", || {
            BackboneShell::stop_learning_sources();
        });
        let midi_receiver = self.request_next_midi_messages();
        let osc_receiver = self.request_next_osc_messages();
        loop {
            let next_capture_result = tokio::select! {
                Ok(r) = midi_receiver.recv() => {
                    Some(MessageCaptureResult::Midi(r))
                }
                Ok(r) = osc_receiver.recv() => {
                    Some(MessageCaptureResult::Osc(r))
                }
                else => None
            };
            if let Some(r) = next_capture_result {
                if let Some((session, mapping)) =
                    self.find_first_relevant_session_with_source_matching(compartment, &r)
                {
                    self.close_message_panel();
                    session
                        .borrow()
                        .show_mapping(compartment, mapping.borrow().id());
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    async fn find_first_mapping_by_target_async(
        &self,
        compartment: CompartmentKind,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some targets!", || {
            BackboneShell::get()
                .control_surface_main_task_sender
                .stop_capturing_targets(None);
        });
        let receiver = self.control_surface_main_task_sender.capture_targets(None);
        while let Ok(event) = receiver.recv().await {
            if event.caused_by_realearn {
                continue;
            }
            if let Some((session, mapping)) =
                self.find_first_relevant_session_with_target(compartment, &event.target)
            {
                self.close_message_panel();
                session
                    .borrow()
                    .show_mapping(compartment, mapping.borrow().id());
            }
        }
        Ok(())
    }

    async fn learn_mapping_reassigning_source_async(
        &self,
        compartment: CompartmentKind,
        open_mapping: bool,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        if self.find_first_relevant_session_project_first().is_none() {
            self.close_message_panel_with_alert(
                "At first you need to add a ReaLearn instance to the monitoring FX chain or this project! Don't forget to set the MIDI control input.",
            );
            return Err("no ReaLearn unit");
        }
        let capture_result = self
            .prompt_for_next_message("Touch a control element!")
            .await?;
        let session = if let Some(s) = capture_result
            .to_input_descriptor(false)
            .and_then(|id| self.find_first_relevant_session_with_input_from(&id))
        {
            s
        } else {
            self.close_message_panel_with_alert(
                "No ReaLearn unit found which has this MIDI control input! First please add one to the monitoring FX chain or this project and set the MIDI control input accordingly!",
            );
            return Err("no ReaLearn unit with that MIDI input");
        };
        let reaper_target = self
            .prompt_for_next_reaper_target("Now touch the desired target!")
            .await?;
        self.close_message_panel();
        let (session, mapping) = if let Some((session, mapping)) =
            self.find_first_relevant_session_with_source_matching(compartment, &capture_result)
        {
            // There's already a mapping with that source. Change target of that mapping.
            {
                let mut m = mapping.borrow_mut();
                session.borrow_mut().change_target_with_closure(
                    &mut m,
                    None,
                    Rc::downgrade(&session),
                    |ctx| {
                        ctx.mapping.target_model.apply_from_target(
                            &reaper_target,
                            ctx.extended_context,
                            compartment,
                        )
                    },
                );
            }
            (session, mapping)
        } else {
            // There's no mapping with that source yet. Add it to the previously determined first
            // session.
            let mapping = {
                let mut s = session.borrow_mut();
                let mapping = s.add_default_mapping(
                    compartment,
                    GroupId::default(),
                    VirtualControlElementType::Multi,
                );
                let mut m = mapping.borrow_mut();
                let event = MessageCaptureEvent {
                    result: capture_result,
                    allow_virtual_sources: true,
                    osc_arg_index_hint: None,
                };
                let compound_source = s
                    .create_compound_source(event)
                    .ok_or("couldn't create compound source")?;
                let _ = m.source_model.apply_from_source(&compound_source);
                let _ = m.target_model.apply_from_target(
                    &reaper_target,
                    s.extended_context(),
                    compartment,
                );
                drop(m);
                mapping
            };
            (session, mapping)
        };
        if open_mapping {
            session
                .borrow()
                .show_mapping(compartment, mapping.borrow().id());
        }
        Ok(())
    }

    fn close_message_panel_with_alert(&self, msg: &str) {
        self.close_message_panel();
        notification::alert(msg);
    }

    fn toggle_guard(&self) -> Result<(), &'static str> {
        if self.message_panel.is_open() {
            self.close_message_panel();
            return Err("a message panel action was already executing, cancelled it");
        }
        // Continue
        Ok(())
    }

    async fn prompt_for_next_message(
        &self,
        msg: &str,
    ) -> Result<MessageCaptureResult, &'static str> {
        self.show_message_panel("ReaLearn", msg, || {
            BackboneShell::stop_learning_sources();
        });
        let midi_receiver = self.request_next_midi_messages();
        let osc_receiver = self.request_next_osc_messages();
        tokio::select! {
            Ok(r) = midi_receiver.recv() => {
                Ok(MessageCaptureResult::Midi(r))
            }
            Ok(r) = osc_receiver.recv() => {
                Ok(MessageCaptureResult::Osc(r))
            }
            else => Err("stopped learning")
        }
    }

    fn stop_learning_sources() {
        BackboneShell::get()
            .audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::StopCapturingMidi);
        BackboneShell::get()
            .control_surface_main_task_sender
            .0
            .send_complaining(RealearnControlSurfaceMainTask::StopCapturingOsc);
    }

    fn request_next_midi_messages(&self) -> async_channel::Receiver<MidiScanResult> {
        let (sender, receiver) = async_channel::bounded(500);
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::StartCapturingMidi(sender));
        receiver
    }

    fn request_next_osc_messages(&self) -> async_channel::Receiver<OscScanResult> {
        let (sender, receiver) = async_channel::bounded(500);
        self.control_surface_main_task_sender
            .0
            .send_complaining(RealearnControlSurfaceMainTask::StartCapturingOsc(sender));
        receiver
    }

    pub async fn request_midi_device_identity(
        &self,
        output_device_id: MidiOutputDeviceId,
        input_device_id: Option<MidiInputDeviceId>,
    ) -> Result<RequestMidiDeviceIdentityReply, &'static str> {
        let reply_receiver =
            self.request_midi_device_identity_internal(output_device_id, input_device_id);
        reply_receiver
            .recv()
            .await
            .map_err(|_| "no device reply received")
    }

    fn request_midi_device_identity_internal(
        &self,
        output_device_id: MidiOutputDeviceId,
        input_device_id: Option<MidiInputDeviceId>,
    ) -> async_channel::Receiver<RequestMidiDeviceIdentityReply> {
        let (sender, receiver) = async_channel::bounded(10);
        let command = RequestMidiDeviceIdentityCommand {
            output_device_id,
            input_device_id,
            sender,
        };
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::RequestMidiDeviceIdentity(command));
        receiver
    }

    async fn prompt_for_next_reaper_target(&self, msg: &str) -> Result<ReaperTarget, &'static str> {
        self.show_message_panel("ReaLearn", msg, || {
            BackboneShell::get()
                .control_surface_main_task_sender
                .stop_capturing_targets(None);
        });
        let receiver = self.control_surface_main_task_sender.capture_targets(None);
        while let Ok(event) = receiver.recv().await {
            if event.caused_by_realearn {
                continue;
            }
            return Ok(event.target);
        }
        Err("capturing ended")
    }

    fn start_learning_source_for_target(
        &self,
        compartment: CompartmentKind,
        target: &ReaperTarget,
    ) {
        // Try to find an existing session which has a target with that parameter
        let session = self
            .find_first_relevant_session_with_target(compartment, target)
            .map(|(s, _)| s)
            // If not found, find the instance on the parameter's track (if there's one)
            .or_else(|| {
                target
                    .track()
                    .and_then(|t| self.find_first_session_on_track(t))
            })
            // If not found, find a random instance
            .or_else(|| self.find_first_relevant_session_project_first());
        match session {
            None => {
                notification::alert(
                    "No suitable ReaLearn unit found! First please add one to the monitoring FX chain or this project!",
                );
            }
            Some(s) => {
                let mapping =
                    s.borrow_mut()
                        .toggle_learn_source_for_target(&s, compartment, target, false);
                s.borrow().show_mapping(compartment, mapping.borrow().id());
            }
        }
    }

    fn find_first_relevant_session_with_target(
        &self,
        compartment: CompartmentKind,
        target: &ReaperTarget,
    ) -> Option<(SharedUnitModel, SharedMapping)> {
        self.find_first_session_with_target(
            Some(Reaper::get().current_project()),
            compartment,
            target,
        )
        .or_else(|| self.find_first_session_with_target(None, compartment, target))
    }

    fn find_first_session_with_target(
        &self,
        project: Option<Project>,
        compartment: CompartmentKind,
        target: &ReaperTarget,
    ) -> Option<(SharedUnitModel, SharedMapping)> {
        self.unit_infos.borrow().iter().find_map(|session| {
            let session = session.unit_model.upgrade()?;
            let mapping = {
                let s = session.borrow();
                if s.processor_context().project() != project {
                    return None;
                }
                s.find_mapping_with_target(compartment, target)?.clone()
            };
            Some((session, mapping))
        })
    }

    fn find_first_session_on_track(&self, track: &Track) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().track() == Some(track)
        })
    }

    fn find_first_relevant_session_monitoring_first(&self) -> Option<SharedUnitModel> {
        self.find_first_session_in_project(None)
            .or_else(|| self.find_first_session_in_project(Some(Reaper::get().current_project())))
    }

    fn find_first_relevant_session_project_first(&self) -> Option<SharedUnitModel> {
        self.find_first_session_in_project(Some(Reaper::get().current_project()))
            .or_else(|| self.find_first_session_in_project(None))
    }

    /// Project None means monitoring FX chain.
    fn find_first_session_in_project(&self, project: Option<Project>) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().project() == project
        })
    }

    fn find_first_relevant_session_with_input_from(
        &self,
        input_descriptor: &InputDescriptor,
    ) -> Option<SharedUnitModel> {
        self.find_first_session_with_input_from(
            Some(Reaper::get().current_project()),
            input_descriptor,
        )
        .or_else(|| self.find_first_session_with_input_from(None, input_descriptor))
    }

    fn find_first_session_with_input_from(
        &self,
        project: Option<Project>,
        input_descriptor: &InputDescriptor,
    ) -> Option<SharedUnitModel> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().project() == project
                && session.receives_input_from(input_descriptor)
        })
    }

    fn find_first_relevant_session_with_source_matching(
        &self,
        compartment: CompartmentKind,
        capture_result: &MessageCaptureResult,
    ) -> Option<(SharedUnitModel, SharedMapping)> {
        self.find_first_session_with_source_matching(
            Some(Reaper::get().current_project()),
            compartment,
            capture_result,
        )
        .or_else(|| self.find_first_session_with_source_matching(None, compartment, capture_result))
    }

    fn find_first_session_with_source_matching(
        &self,
        project: Option<Project>,
        compartment: CompartmentKind,
        capture_result: &MessageCaptureResult,
    ) -> Option<(SharedUnitModel, SharedMapping)> {
        self.unit_infos.borrow().iter().find_map(|session| {
            let session = session.unit_model.upgrade()?;
            let mapping = {
                let s = session.borrow();
                if s.processor_context().project() != project {
                    return None;
                }
                let input_descriptor = capture_result.to_input_descriptor(true)?;
                if !s.receives_input_from(&input_descriptor) {
                    return None;
                }
                s.find_mapping_with_source(compartment, capture_result.message())?
                    .clone()
            };
            Some((session, mapping))
        })
    }

    fn server_resource_dir_path() -> PathBuf {
        Self::helgoboss_resource_dir_path().join("Server")
    }

    fn notify_changed(&self) {
        self.sessions_changed_subject.borrow_mut().next(());
    }

    fn do_with_initiator_session_or_sessions_matching_tags(
        &self,
        common_args: &InstanceContainerCommonArgs,
        f: impl Fn(&mut UnitModel, WeakUnitModel),
    ) -> Result<(), &'static str> {
        if common_args.scope.has_tags() {
            // Modify all sessions whose tags match.
            for instance in self.unit_infos.borrow().iter() {
                if let Some(session) = instance.unit_model.upgrade() {
                    let mut session = session.borrow_mut();
                    // Don't leave the context (project if in project, FX chain if monitoring FX).
                    let context = session.processor_context();
                    if context.project() != common_args.initiator_project {
                        continue;
                    }
                    // Skip unmatched tags.
                    let session_tags = session.tags.get_ref();
                    if !common_args.scope.any_tag_matches(session_tags) {
                        continue;
                    }
                    f(&mut session, instance.unit_model.clone())
                }
            }
        } else {
            // Modify the initiator session only.
            let shared_session = self
                .find_unit_model_by_unit_id_ignoring_borrowed_ones(
                    common_args.initiator_instance_id,
                )
                .ok_or("initiator session not found")?;
            let mut session = shared_session.borrow_mut();
            f(&mut session, Rc::downgrade(&shared_session));
        }
        Ok(())
    }
}

impl Drop for BackboneShell {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BackboneConfig {
    main: MainConfig,
    toolbar: HashMap<String, u8>,
}

impl BackboneConfig {
    pub fn load() -> Result<BackboneConfig, String> {
        let ini_content = fs::read_to_string(Self::config_file_path())
            .map_err(|_| "couldn't read config file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{e:?}"))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let ini_content = serde_ini::to_string(self).map_err(|_| "couldn't serialize config")?;
        let config_file_path = Self::config_file_path();
        fs::create_dir_all(config_file_path.parent().unwrap())
            .expect("couldn't create config directory");
        fs::write(config_file_path, ini_content).map_err(|_| "couldn't write config file")?;
        Ok(())
    }

    pub fn enable_server(&mut self) {
        self.main.server_enabled = 1;
    }

    pub fn disable_server(&mut self) {
        self.main.server_enabled = 0;
    }

    pub fn server_is_enabled(&self) -> bool {
        self.main.server_enabled > 0
    }

    pub fn companion_web_app_url(&self) -> url::Url {
        Url::parse(&self.main.companion_web_app_url).expect("invalid companion web app URL")
    }

    pub fn toolbar_button_is_enabled(&self, command_name: &str) -> bool {
        self.toolbar.get(command_name).is_some_and(|v| *v != 0)
    }

    fn config_file_path() -> PathBuf {
        BackboneShell::realearn_resource_dir_path().join("realearn.ini")
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MainConfig {
    #[serde(default, skip_serializing_if = "is_default")]
    server_enabled: u8,
    #[serde(
        default = "default_server_http_port",
        skip_serializing_if = "is_default_server_http_port"
    )]
    server_http_port: u16,
    #[serde(
        default = "default_server_https_port",
        skip_serializing_if = "is_default_server_https_port"
    )]
    server_https_port: u16,
    #[serde(
        default = "default_server_grpc_port",
        skip_serializing_if = "is_default_server_grpc_port"
    )]
    server_grpc_port: u16,
    #[serde(
        default = "default_companion_web_app_url",
        skip_serializing_if = "is_default_companion_web_app_url"
    )]
    companion_web_app_url: String,
    showed_welcome_screen: u8,
}

const DEFAULT_SERVER_HTTP_PORT: u16 = 39080;
const DEFAULT_SERVER_HTTPS_PORT: u16 = 39443;
const DEFAULT_SERVER_GRPC_PORT: u16 = 39051;

fn default_server_http_port() -> u16 {
    DEFAULT_SERVER_HTTP_PORT
}

fn is_default_server_http_port(v: &u16) -> bool {
    *v == DEFAULT_SERVER_HTTP_PORT
}

fn default_server_https_port() -> u16 {
    DEFAULT_SERVER_HTTPS_PORT
}

fn is_default_server_https_port(v: &u16) -> bool {
    *v == DEFAULT_SERVER_HTTPS_PORT
}

fn default_server_grpc_port() -> u16 {
    DEFAULT_SERVER_GRPC_PORT
}

fn is_default_server_grpc_port(v: &u16) -> bool {
    *v == DEFAULT_SERVER_GRPC_PORT
}

fn default_companion_web_app_url() -> String {
    COMPANION_WEB_APP_URL.to_string()
}

fn is_default_companion_web_app_url(v: &str) -> bool {
    v == COMPANION_WEB_APP_URL
}

impl Default for MainConfig {
    fn default() -> Self {
        MainConfig {
            server_enabled: 0,
            server_http_port: default_server_http_port(),
            server_https_port: default_server_https_port(),
            server_grpc_port: default_server_grpc_port(),
            companion_web_app_url: default_companion_web_app_url(),
            showed_welcome_screen: 0,
        }
    }
}

fn build_detailed_version() -> String {
    use crate::infrastructure::plugin::built_info::*;
    let dirty_mark = if GIT_DIRTY == Some(true) {
        "-dirty"
    } else {
        ""
    };
    let date_info = if let Ok(d) = chrono::DateTime::parse_from_rfc2822(BUILT_TIME_UTC) {
        d.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        BUILT_TIME_UTC.to_string()
    };
    let debug_mark = if PROFILE == "debug" { "-debug" } else { "" };
    let hash = GIT_COMMIT_HASH
        .map(|h| h[0..6].to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("v{PKG_VERSION}/{CFG_TARGET_ARCH}{debug_mark} rev {hash}{dirty_mark} ({date_info})")
}

pub fn warn_about_failed_server_start(info: String) {
    Reaper::get().show_console_msg(format!(
        "Couldn't start ReaLearn projection server because {info}"
    ))
}

fn determine_module_base_address() -> Option<usize> {
    let hinstance = Reaper::get()
        .medium_reaper()
        .plugin_context()
        .h_instance()?;
    Some(hinstance.as_ptr() as usize)
}

impl HookPostCommand for BackboneShell {
    fn call(command_id: CommandId, _flag: i32) {
        BackboneShell::get()
            .additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                section_context: SectionContext::MainSection,
                command_id,
            }));
    }
}

impl HookPostCommand2 for BackboneShell {
    fn call(
        section: SectionContext,
        command_id: CommandId,
        _value_change: ActionValueChange,
        _window: WindowContext,
        _project: ReaProject,
    ) {
        if section != SectionContext::MainSection {
            return;
        }
        // Process executed action as feedback
        BackboneShell::get()
            .additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                section_context: SectionContext::MainSection,
                command_id,
            }));
        #[cfg(feature = "playtime")]
        post_process_action_invocation_for_playtime(command_id);
    }
}

#[cfg(feature = "playtime")]
fn post_process_action_invocation_for_playtime(command_id: CommandId) {
    let toggle_metronome_command_id = CommandId::new(40364);
    if command_id == toggle_metronome_command_id {
        // Metronome toggle
        let toggle_metronome_action = Reaper::get()
            .main_section()
            .action_by_command_id(command_id);
        if toggle_metronome_action.is_on() == Ok(Some(false)) {
            // Switched metronome off. Switch Playtime clicks off as well!
            BackboneShell::get().with_instance_shell_infos(|infos| {
                for instance in infos.iter().flat_map(|info| info.instance.upgrade()) {
                    let mut instance = instance.borrow_mut();
                    if let Some(matrix) = instance.clip_matrix_mut() {
                        matrix.set_click_enabled(false);
                    }
                }
            });
        }
    }
}

impl UnitContainer for BackboneShell {
    fn find_session_by_id(&self, session_id: &str) -> Option<SharedUnitModel> {
        BackboneShell::get().find_session_by_id_ignoring_borrowed_ones(session_id)
    }

    fn find_session_by_instance_id(&self, instance_id: UnitId) -> Option<SharedUnitModel> {
        BackboneShell::get().find_unit_model_by_unit_id_ignoring_borrowed_ones(instance_id)
    }

    fn enable_instances(&self, args: EnableInstancesArgs) -> Option<NonCryptoHashSet<Tag>> {
        let mut activated_inverse_tags = HashSet::default();
        for session in self.unit_infos.borrow().iter() {
            if let Some(session) = session.unit_model.upgrade() {
                let session = session.borrow();
                // Don't touch ourselves.
                if session.unit_id() == args.common.initiator_instance_id {
                    continue;
                }
                // Don't leave the context (project if in project, FX chain if monitoring FX).
                let context = session.processor_context();
                if context.project() != args.common.initiator_project {
                    continue;
                }
                // Determine how to change the instances.
                let session_tags = session.tags.get_ref();
                let flag = match args.common.scope.determine_enable_disable_change(
                    args.exclusivity,
                    session_tags,
                    args.is_enable,
                ) {
                    None => continue,
                    Some(f) => f,
                };
                if args.exclusivity == Exclusivity::Exclusive && !args.is_enable {
                    // Collect all *other* instance tags because they are going to be activated
                    // and we have to know about them!
                    activated_inverse_tags.extend(session_tags.iter().cloned());
                }
                let enable = if args.is_enable { flag } else { !flag };
                let fx = context.containing_fx();
                if enable {
                    fx.enable();
                } else {
                    fx.disable();
                }
            }
        }
        if args.exclusivity == Exclusivity::Exclusive && !args.is_enable {
            Some(activated_inverse_tags)
        } else {
            None
        }
    }

    fn change_instance_fx(&self, args: ChangeInstanceFxArgs) -> Result<(), &'static str> {
        // At first create the FX descriptor that we want to set/pin in the destination sessions.
        let fx_descriptor = match args.request {
            InstanceFxChangeRequest::Pin {
                track_guid,
                is_input_fx,
                fx_guid,
            } => {
                let track_desc = convert_optional_guid_to_api_track_descriptor(track_guid);
                let chain_desc = if is_input_fx {
                    TrackFxChain::Input
                } else {
                    TrackFxChain::Normal
                };
                FxDescriptor::ById {
                    commons: Default::default(),
                    chain: FxChainDescriptor::Track {
                        track: Some(track_desc),
                        chain: Some(chain_desc),
                    },
                    id: Some(fx_guid.to_string_without_braces()),
                }
            }
            InstanceFxChangeRequest::SetFromMapping(id) => {
                let mapping = self.find_original_mapping(args.common.initiator_instance_id, id)?;
                let mapping = mapping.borrow();
                mapping.target_model.api_fx_descriptor()
            }
        };
        self.do_with_initiator_session_or_sessions_matching_tags(
            &args.common,
            |session, weak_session| {
                session.change_with_notification(
                    SessionCommand::SetInstanceFx(fx_descriptor.clone()),
                    None,
                    weak_session,
                );
            },
        )
    }

    fn change_instance_track(&self, args: ChangeInstanceTrackArgs) -> Result<(), &'static str> {
        // At first create the track descriptor that we want to set/pin in the destination sessions.
        let track_descriptor = match args.request {
            InstanceTrackChangeRequest::Pin(guid) => {
                convert_optional_guid_to_api_track_descriptor(guid)
            }
            InstanceTrackChangeRequest::SetFromMapping(id) => {
                let mapping = self.find_original_mapping(args.common.initiator_instance_id, id)?;
                let mapping = mapping.borrow();
                mapping.target_model.api_track_descriptor()
            }
        };
        self.do_with_initiator_session_or_sessions_matching_tags(
            &args.common,
            |session, weak_session| {
                session.change_with_notification(
                    SessionCommand::SetInstanceTrack(track_descriptor.clone()),
                    None,
                    weak_session,
                );
            },
        )
    }
}

fn convert_optional_guid_to_api_track_descriptor(guid: Option<Guid>) -> TrackDescriptor {
    if let Some(guid) = guid {
        TrackDescriptor::ById {
            commons: Default::default(),
            id: Some(guid.to_string_without_braces()),
        }
    } else {
        TrackDescriptor::Master {
            commons: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct BackboneHelgoboxWindowSnitch;

impl HelgoboxWindowSnitch for BackboneHelgoboxWindowSnitch {
    fn find_closest_realearn_view(&self, window: Window) -> Option<SharedView<dyn View>> {
        let view_manager = ViewManager::get();
        let mut current_window = Some(window);
        while let Some(w) = current_window {
            if let Some(v) = view_manager.get_associated_view(w) {
                // It's one of our views!
                return Some(v);
            }
            current_window = w.parent();
        }
        None
    }
}

#[derive(Debug)]
pub struct BackboneControlSurfaceEventHandler;

impl ControlSurfaceEventHandler for BackboneControlSurfaceEventHandler {
    fn midi_input_devices_changed(
        &self,
        _diff: &DeviceDiff<MidiInputDeviceId>,
        _device_config_changed: bool,
    ) {
        BackboneShell::get()
            .proto_hub()
            .notify_midi_input_devices_changed();
        update_auto_units_async();
    }

    fn midi_output_devices_changed(
        &self,
        diff: &DeviceDiff<MidiOutputDeviceId>,
        _device_config_changed: bool,
    ) {
        BackboneShell::get()
            .proto_hub()
            .notify_midi_output_devices_changed();
        update_auto_units_async();
        // Maybe create controller for each added device
        let added_devices = diff.added_devices.clone();
        spawn_in_main_thread(async move {
            // Prevent multiple tasks of this kind from running at the same time
            static IS_RUNNING: AtomicBool = AtomicBool::new(false);
            if IS_RUNNING.swap(true, Ordering::Relaxed) {
                return Ok(());
            }
            // Now, let's go
            for out_dev_id in added_devices.iter().copied() {
                if let Err(error) = maybe_create_controller_for_device(out_dev_id).await {
                    tracing::warn!(msg = "Couldn't automatically create controller for device", %out_dev_id, %error);
                }
            }
            IS_RUNNING.store(false, Ordering::Relaxed);
            Ok(())
        });
    }

    fn process_reaper_change_events(&self, change_events: &[ChangeEvent]) {
        BackboneShell::get()
            .proto_hub
            .send_global_events_caused_by_reaper_change_events(change_events);
    }
}

#[derive(Debug)]
pub struct BackboneMainPresetManagerEventHandler;

impl CompartmentPresetManagerEventHandler for BackboneMainPresetManagerEventHandler {
    type Source = FileBasedMainPresetManager;

    fn presets_changed(&self, source: &Self::Source) {
        BackboneShell::get()
            .proto_hub()
            .notify_main_presets_changed(source);
    }
}

#[derive(Debug)]
pub struct BackboneControllerManagerEventHandler;

impl ControllerManagerEventHandler for BackboneControllerManagerEventHandler {
    fn controller_config_changed(&self, source: &ControllerManager) {
        tracing::debug!("Controller config changed");
        BackboneShell::get()
            .proto_hub()
            .notify_controller_config_changed(source);
        update_auto_units_async();
    }
}

#[derive(Debug)]
pub struct BackboneLicenseManagerEventHandler;

impl LicenseManagerEventHandler for BackboneLicenseManagerEventHandler {
    fn licenses_changed(&self, source: &LicenseManager) {
        let shell = BackboneShell::get();
        // Inform Playtime, currently the only module which contains license-only functions
        #[cfg(feature = "playtime")]
        {
            // Let the Playtime Clip Engine check if it finds a suitable license
            let success = playtime_clip_engine::PlaytimeEngine::get()
                .handle_changed_licenses(source.licenses());
            // Send a notification to the app (if it wants to display "success")
            let info_event = if success {
                GlobalInfoEvent::PlaytimeActivationSucceeded
            } else {
                GlobalInfoEvent::PlaytimeActivationFailed
            };
            shell.proto_hub().notify_about_global_info_event(info_event);
            // Give all Playtime instances a chance to load previously unloaded matrices
            shell.with_instance_shell_infos(|infos| {
                for instance in infos.iter().filter_map(|info| info.instance.upgrade()) {
                    let mut instance = instance.borrow_mut();
                    if let Some(matrix) = instance.clip_matrix_mut() {
                        let result = matrix.notify_license_state_changed();
                        notification::notify_user_on_anyhow_error(result);
                    }
                }
            });
        }
        // Broadcast new license list (if the app wants to display it)
        shell.proto_hub().notify_licenses_changed(source);
    }
}

#[derive(Debug)]
pub struct BackboneControllerPresetManagerEventHandler;

impl CompartmentPresetManagerEventHandler for BackboneControllerPresetManagerEventHandler {
    type Source = FileBasedControllerPresetManager;

    fn presets_changed(&self, source: &Self::Source) {
        BackboneShell::get()
            .proto_hub()
            .notify_controller_presets_changed(source);
    }
}

fn load_app_library() -> anyhow::Result<crate::infrastructure::ui::AppLibrary> {
    tracing::info!("Loading app library...");
    let app_base_dir = BackboneShell::app_binary_base_dir_path();
    let lib = crate::infrastructure::ui::AppLibrary::load(app_base_dir);
    match lib.as_ref() {
        Ok(_) => {
            tracing::info!("App library loaded successfully");
        }
        Err(e) => {
            tracing::warn!("App library loading failed: {e:#}");
        }
    }
    lib
}

fn decompress_app() -> anyhow::Result<()> {
    // Check if decompression necessary
    let archive_file = &BackboneShell::app_archive_file_path();
    let destination_dir = &BackboneShell::app_binary_base_dir_path();
    let archive_metadata = archive_file.metadata()?;
    let archive_size = archive_metadata.len();
    let archive_modified = archive_metadata
        .modified()?
        .duration_since(std::time::SystemTime::UNIX_EPOCH)?;
    let archive_id = format!("{archive_size},{}", archive_modified.as_millis());
    let archive_id_file = destination_dir.join("ARCHIVE");
    if let Ok(unpacked_archive_id) = fs::read_to_string(&archive_id_file) {
        if archive_id == unpacked_archive_id {
            tracing::info!("App is already decompressed.");
            return Ok(());
        }
    }
    // Decompress
    tracing::info!("Decompressing app...");
    let archive_file =
        fs::File::open(archive_file).context("Couldn't open app archive file. Maybe you installed ReaLearn manually (without ReaPack) and forgot to add the app archive?")?;
    let tar = zstd::Decoder::new(&archive_file).context("Couldn't decode app archive file.")?;
    let mut archive = tar::Archive::new(tar);
    if destination_dir.exists() {
        #[cfg(target_family = "windows")]
            let context = "Couldn't clean up existing app directory. This can happen if you have \"Allow complete unload of VST plug-ins\" enabled in REAPER preferences => Plug-ins => VST. Turn this option off and restart REAPER before using the app.";
        #[cfg(target_family = "unix")]
        let context = "Couldn't remove existing app directory";
        fs::remove_dir_all(destination_dir).context(context)?;
    }
    archive
        .unpack(destination_dir)
        .context("Couldn't unpack app archive.")?;
    fs::write(archive_id_file, archive_id)?;
    tracing::info!("App decompressed successfully");
    Ok(())
}

async fn maybe_create_controller_for_device(
    out_dev_id: MidiOutputDeviceId,
) -> Result<(), Box<dyn Error>> {
    // Don't create controller if there is one already which uses that device
    let output_used_already = BackboneShell::get()
        .controller_manager
        .borrow()
        .find_controller_connected_to_midi_output(out_dev_id)
        .is_some();
    if output_used_already {
        // Output used already by existing controller
        return Ok(());
    }
    // Make sure that MIDI output device is enabled
    tracing::debug!(msg = "Temporarily enabling MIDI output device", %out_dev_id);
    let old_midi_outs = MidiOutDevsConfig::from_reaper();
    let tmp_midi_outs = old_midi_outs.with_dev_enabled(out_dev_id);
    tmp_midi_outs.apply_to_reaper();
    // Temporarily enable all input devices (so they can listen to the device identity reply)
    tracing::debug!(msg = "Temporarily enabling all input devices...");
    let old_midi_ins = MidiInDevsConfig::from_reaper();
    let tmp_midi_ins = MidiInDevsConfig::ALL_ENABLED;
    tmp_midi_ins.apply_to_reaper();
    // Apply changes
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Send device identity request to MIDI output device
    tracing::debug!(msg = "Sending device request to MIDI output device...", %out_dev_id);
    let identity_reply_result = BackboneShell::get()
        .request_midi_device_identity(out_dev_id, None)
        .await;
    // As soon as possible, reset MIDI devices to old state (we don't want to leave traces)
    tracing::debug!(msg = "Resetting MIDI output and input devices to previous state...");
    old_midi_outs.apply_to_reaper();
    old_midi_ins.apply_to_reaper();
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Process identity reply
    let identity_reply = identity_reply_result?;
    let in_dev_id = identity_reply.input_device_id;
    tracing::debug!(
        msg = "Received identity reply from MIDI device",
        %out_dev_id,
        %in_dev_id,
        reply = %identity_reply.device_inquiry_reply
    );
    //  Check if input already used by existing controller
    tracing::debug!(msg = "Check if input used already used by existing controller...");
    let input_used_already = BackboneShell::get()
        .controller_manager
        .borrow()
        .find_controller_connected_to_midi_input(in_dev_id)
        .is_some();
    if input_used_already {
        // Input already used by existing controller
        tracing::debug!(msg = "Input already used");
        return Ok(());
    }
    // Neither output nor input used already. Maybe this is a known controller!
    let controller_preset_manager = BackboneShell::get().controller_preset_manager().borrow();
    let out_port_name = Reaper::get()
        .midi_output_device_by_id(out_dev_id)
        .name()
        .ok_or("MIDI output device doesn't return name / is not available")?
        .into_string();
    tracing::debug!(msg = "Input not yet used. Finding matching controller preset...", %out_port_name);
    let controller_preset = controller_preset_manager
        .find_controller_preset_compatible_with_device(
            &identity_reply.device_inquiry_reply.message,
            &out_port_name,
        )
        .ok_or("no controller preset matching device")?;
    let device_name = controller_preset
        .specific_meta_data
        .device_name
        .as_ref()
        .ok_or("controller preset doesn't have device name")?;
    // Search for suitable main preset
    let controller_preset_id = &controller_preset.common.id;
    tracing::debug!(msg = "Found controller preset", %controller_preset_id);
    tracing::debug!(msg = "Finding main preset...");
    let main_preset_manager = BackboneShell::get().main_preset_manager().borrow();
    let conditions = MainPresetSelectionConditions {
        at_least_one_instance_has_playtime_clip_matrix: {
            BackboneShell::get()
                .find_first_helgobox_instance_matching(|info| {
                    let Some(instance) = info.instance.upgrade() else {
                        return false;
                    };
                    let instance_state = instance.borrow();
                    instance_state.has_clip_matrix()
                })
                .is_some()
        },
    };
    let main_preset = main_preset_manager.find_most_suitable_main_preset_for_schemes(
        &controller_preset.specific_meta_data.provided_schemes,
        conditions,
    );
    tracing::debug!(msg = "Main preset result available", ?main_preset);
    let default_main_preset = main_preset.map(|mp| CompartmentPresetId::new(mp.common.id.clone()));
    // Make sure the involved MIDI devices are enabled
    tracing::debug!(
        "Enabling MIDI input device {in_dev_id} and MIDI output device {out_dev_id}..."
    );
    let new_midi_outs = old_midi_outs.with_dev_enabled(out_dev_id);
    new_midi_outs.apply_to_reaper();
    let new_midi_ins = old_midi_ins.with_dev_enabled(in_dev_id);
    new_midi_ins.apply_to_reaper();
    Reaper::get().medium_reaper().low().midi_init(-1, -1);
    // Persist the changes
    tracing::debug!(
        msg = "Persisting MIDI in/out dev changes...",
        ?new_midi_ins,
        ?new_midi_outs
    );
    write_midi_devs_config_to_reaper_ini(new_midi_ins, new_midi_outs)?;
    // Auto-create controller
    tracing::debug!("Auto-creating controller...");
    let controller = Controller {
        id: "".to_string(),
        name: device_name.clone(),
        enabled: true,
        palette_color: None,
        connection: Some(ControllerConnection::Midi(MidiControllerConnection {
            identity_response: Some(format_as_pretty_hex(
                &identity_reply.device_inquiry_reply.message,
            )),
            input_port: Some(MidiInputPort::new(
                identity_reply.input_device_id.get() as u32
            )),
            output_port: Some(MidiOutputPort::new(out_dev_id.get() as u32)),
        })),
        default_controller_preset: None,
        default_main_preset,
    };
    let outcome = BackboneShell::get()
        .controller_manager
        .borrow_mut()
        .save_controller(controller)?;
    BackboneShell::get()
        .proto_hub()
        .notify_about_global_info_event(GlobalInfoEvent::AutoAddedController(
            AutoAddedControllerEvent {
                controller_id: outcome.id,
            },
        ));
    Ok(())
}

fn write_midi_devs_config_to_reaper_ini(
    midi_in_devs: MidiInDevsConfig,
    midi_out_devs: MidiOutDevsConfig,
) -> anyhow::Result<()> {
    let reaper = Reaper::get();
    let reaper_ini = reaper.medium_reaper().get_ini_file(|p| p.to_path_buf());
    let reaper_ini = reaper_ini.to_str().context("non-UTF8 path")?;
    // Replace existing entries
    for (key, val) in midi_in_devs
        .to_ini_entries()
        .chain(midi_out_devs.to_ini_entries())
    {
        ini_util::write_ini_entry(reaper_ini, "REAPER", key, val.to_string())?;
    }
    Ok(())
}

impl HookCustomMenu for BackboneShell {
    fn call(menuidstr: &ReaperStr, menu: Hmenu, flag: MenuHookFlag) {
        if flag != MenuHookFlag::Init || menuidstr.to_str() != "Main extensions" {
            return;
        }
        let swell_menu = Menu::new(menu.as_ptr());
        let pure_menu = menus::extension_menu();
        swell_ui::menu_tree::add_all_entries_of_menu(swell_menu, &pure_menu);
    }
}

impl ToolbarIconMap for BackboneShell {
    fn call(
        _toolbar_name: &ReaperStr,
        command_id: CommandId,
        _toggle_state: Option<bool>,
    ) -> Option<&'static ReaperStr> {
        Reaper::get().with_our_command(command_id, |command| match command?.command_name() {
            ACTION_SHOW_HIDE_PLAYTIME_COMMAND_NAME => Some(reaper_str!("toolbar_playtime")),
            _ => None,
        })
    }
}

fn reaper_window() -> Window {
    Window::from_hwnd(Reaper::get().main_window())
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::infrastructure::data::LicenseManager;
    use crate::infrastructure::plugin::helgobox_plugin::HELGOBOX_UNIQUE_VST_PLUGIN_ADD_STRING;
    use crate::infrastructure::plugin::BackboneShell;
    use anyhow::Context;
    use base::metrics_util::{record_duration_internal, record_occurrence};
    use base::Global;
    use playtime_api::persistence::PlaytimeSettings;
    use reaper_high::{GroupingBehavior, Reaper};
    use reaper_medium::{GangBehavior, InputMonitoringMode, RecordingInput};
    use std::fs;
    use std::path::PathBuf;

    impl BackboneShell {
        pub(crate) fn read_playtime_settings() -> Option<PlaytimeSettings> {
            let json = fs::read_to_string(Self::playtime_settings_file_path()).ok()?;
            serde_json::from_str(&json).ok()
        }

        pub fn playtime_settings_file_path() -> PathBuf {
            Self::playtime_dir_path().join("settings.json")
        }

        pub fn playtime_dir_path() -> PathBuf {
            Self::helgoboss_resource_dir_path().join("Playtime")
        }
    }

    fn add_and_show_playtime() -> anyhow::Result<()> {
        let project = Reaper::get().current_project();
        let track = project.insert_track_at(0)?;
        track.set_name("Playtime");
        track.set_recording_input(Some(RecordingInput::Midi {
            device_id: None,
            channel: None,
        }));
        track.arm(
            false,
            GangBehavior::DenyGang,
            GroupingBehavior::PreventGrouping,
        );
        track.set_input_monitoring_mode(
            InputMonitoringMode::Normal,
            GangBehavior::DenyGang,
            GroupingBehavior::PreventGrouping,
        );
        track
            .normal_fx_chain()
            .add_fx_by_original_name(HELGOBOX_UNIQUE_VST_PLUGIN_ADD_STRING)
            .context("Couldn't add Helgobox. Maybe not installed?")?
            .hide_floating_window();
        // The rest needs to be done async because the instance initializes itself async
        // (because FX not yet available when plug-in instantiated).
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(|| {
                enable_playtime_for_first_helgobox_instance_and_show_it().unwrap();
            })
            .unwrap();
        Ok(())
    }

    pub fn init_clip_engine(license_manager: &LicenseManager) {
        use playtime_clip_engine::PlaytimeEngine;
        #[derive(Debug)]
        struct RealearnMetricsRecorder;
        impl playtime_clip_engine::MetricsRecorder for RealearnMetricsRecorder {
            fn record_duration(&self, id: &'static str, delta: std::time::Duration) {
                record_duration_internal(id, delta);
            }

            fn record_occurrence(&self, id: &'static str) {
                record_occurrence(id);
            }
        }
        let metrics_recorder: Option<playtime_clip_engine::StaticMetricsRecorder> =
            if base::metrics_util::metrics_are_enabled() {
                Some(&RealearnMetricsRecorder)
            } else {
                None
            };
        #[derive(Debug)]
        struct RealearnPlaytimeIntegration;
        impl playtime_clip_engine::PlaytimeIntegration for RealearnPlaytimeIntegration {
            fn export_to_clipboard(
                &self,
                item: &dyn playtime_clip_engine::PlaytimeItem,
            ) -> anyhow::Result<()> {
                let text = crate::infrastructure::ui::lua_serializer::to_string(item)?;
                crate::infrastructure::ui::copy_text_to_clipboard(text);
                Ok(())
            }

            fn changed_settings(&self, settings: PlaytimeSettings) -> anyhow::Result<()> {
                BackboneShell::get()
                    .proto_hub
                    .notify_playtime_settings_changed();
                let json = serde_json::to_string_pretty(&settings)?;
                let settings_path = BackboneShell::playtime_settings_file_path();
                fs::create_dir_all(
                    settings_path
                        .parent()
                        .context("Playtime settings file has not parent")?,
                )?;
                fs::write(settings_path, json)?;
                Ok(())
            }
        }
        let args = playtime_clip_engine::PlaytimeEngineInitArgs {
            available_licenses: license_manager.licenses(),
            settings: BackboneShell::read_playtime_settings(),
            metrics_recorder,
            integration: Box::new(RealearnPlaytimeIntegration),
        };
        PlaytimeEngine::make_available_globally(|| PlaytimeEngine::new(args));
    }

    fn enable_playtime_for_first_helgobox_instance_and_show_it() -> anyhow::Result<()> {
        let plugin_context = Reaper::get().medium_reaper().low().plugin_context();
        // We don't really need to do that via the external API but on the other hand, this is the only
        // example so far where we actually use our exposed API! If we don't "eat our own dogfood", we would have
        // to add integration tests in order to quickly realize if this works or not.
        // TODO-low Add integration tests instead of using API here.
        let helgobox_api = realearn_api::runtime::HelgoboxApiSession::load(plugin_context)
            .context("Couldn't load Helgobox API even after adding Helgobox. Old version?")?;
        let playtime_api = playtime_api::runtime::PlaytimeApiSession::load(plugin_context)
            .context("Couldn't load Playtime API even after adding Helgobox. Old version? Or Helgobox built without Playtime?")?;
        let instance_id = helgobox_api.HB_FindFirstHelgoboxInstanceInProject(std::ptr::null_mut());
        playtime_api.HB_CreateClipMatrix(instance_id);
        playtime_api.HB_ShowOrHidePlaytime(instance_id);
        Ok(())
    }

    pub fn show_or_hide_playtime() -> anyhow::Result<()> {
        let plugin_context = Reaper::get().medium_reaper().low().plugin_context();
        let Some(playtime_api) = playtime_api::runtime::PlaytimeApiSession::load(plugin_context)
        else {
            // Project doesn't have any Helgobox instance yet. Add one.
            add_and_show_playtime()?;
            return Ok(());
        };
        let helgobox_instance =
            playtime_api.HB_FindFirstPlaytimeHelgoboxInstanceInProject(std::ptr::null_mut());
        if helgobox_instance == -1 {
            // Project doesn't have any Playtime-enabled Helgobox instance yet. Add one.
            add_and_show_playtime()?;
            return Ok(());
        }
        playtime_api.HB_ShowOrHidePlaytime(helgobox_instance);
        Ok(())
    }
}
