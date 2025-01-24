use crate::application::{
    RealearnControlSurfaceMainTaskSender, SessionCommand, SharedMapping, SharedUnitModel,
    UnitModel, WeakUnitModel,
};
use crate::base::notification;
use crate::domain::{
    ActionInvokedEvent, AdditionalFeedbackEvent, Backbone, ChangeInstanceFxArgs,
    ChangeInstanceTrackArgs, CompartmentKind, ControlSurfaceEventHandler, DeviceDiff,
    EnableInstancesArgs, Exclusivity, FeedbackAudioHookTask, GroupId, HelgoboxWindowSnitch,
    InputDescriptor, InstanceContainerCommonArgs, InstanceFxChangeRequest, InstanceId,
    InstanceTrackChangeRequest, LastTouchedTargetFilter, MainProcessor, MessageCaptureEvent,
    MessageCaptureResult, MidiScanResult, NormalAudioHookTask, OscDeviceId, OscFeedbackProcessor,
    OscFeedbackTask, OscScanResult, ProcessorContext, QualifiedInstanceEvent, QualifiedMappingId,
    RealearnAccelerator, RealearnAudioHook, RealearnControlSurfaceMainTask,
    RealearnControlSurfaceMiddleware, RealearnTarget, RealearnTargetState, ReaperTarget,
    ReaperTargetType, RequestMidiDeviceIdentityCommand, RequestMidiDeviceIdentityReply,
    SharedInstance, SharedMainProcessors, SharedRealTimeProcessor, Tag, UnitContainer, UnitId,
    UnitOrchestrationEvent, WeakInstance, WeakUnit, GLOBAL_AUDIO_STATE,
};
use crate::infrastructure::data::{
    CommonCompartmentPresetManager, CompartmentPresetManagerEventHandler, ControllerManager,
    ControllerManagerEventHandler, FileBasedControllerPresetManager, FileBasedMainPresetManager,
    FileBasedPresetLinkManager, LicenseManager, LicenseManagerEventHandler, OscDevice,
    OscDeviceManager, SharedControllerManager, SharedControllerPresetManager, SharedLicenseManager,
    SharedMainPresetManager, SharedOscDeviceManager, SharedPresetLinkManager,
};
use crate::infrastructure::server;
use crate::infrastructure::server::{
    MetricsReporter, RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL,
};
use crate::infrastructure::ui::{
    app_window_is_in_text_entry_mode, is_app_window, menus, MessagePanel,
};
use base::default_util::is_default;
use base::{
    make_available_globally_in_main_thread_on_demand, spawn_in_main_thread, Global,
    NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread,
};

use crate::base::allocator::{RealearnAllocatorIntegration, RealearnDeallocator, GLOBAL_ALLOCATOR};
use crate::base::notification::notify_user_about_anyhow_error;
use crate::infrastructure::plugin::actions::ACTION_DEFS;
use crate::infrastructure::plugin::api_impl::{register_api, unregister_api};
use crate::infrastructure::plugin::debug_util::resolve_symbols_from_clipboard;
use crate::infrastructure::plugin::dynamic_toolbar::{
    add_or_remove_toolbar_button, custom_toolbar_api_is_available, ToolbarChangeDetector,
};
use crate::infrastructure::plugin::helgobox_plugin::HELGOBOX_UNIQUE_VST_PLUGIN_ADD_STRING;
use crate::infrastructure::plugin::hidden_helper_panel::HiddenHelperPanel;
use crate::infrastructure::plugin::tracing_util::TracingHook;
use crate::infrastructure::plugin::{
    built_info, controller_detection, sentry, update_auto_units_async, SharedInstanceShell,
    WeakInstanceShell,
};
use crate::infrastructure::server::services::Services;
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::util::open_child_panel;
use crate::infrastructure::ui::welcome_panel::WelcomePanel;
use anyhow::{anyhow, bail, Context};
use base::future_util::millis;
use base::hash_util::NonCryptoHashSet;
use base::metrics_util::MetricsHook;
use camino::{Utf8Path, Utf8PathBuf};
use helgobox_allocator::{start_async_deallocation_thread, AsyncDeallocatorCommandReceiver};
use helgobox_api::persistence::{
    Envelope, FxChainDescriptor, FxDescriptor, TargetTouchCause, TrackDescriptor, TrackFxChain,
    VirtualControlElementCharacter,
};
use itertools::Itertools;
use once_cell::sync::Lazy;
use reaper_high::{
    ChangeEvent, Fx, Guid, MiddlewareControlSurface, PluginInfo, Project, Reaper, Track,
};
use reaper_low::{raw, register_plugin_destroy_hook, PluginContext, PluginDestroyHook, Swell};
use reaper_macros::reaper_extension_plugin;
use reaper_medium::{
    AccelMsg, AcceleratorPosition, ActionValueChange, CommandId, Hmenu, HookCustomMenu,
    HookPostCommand, HookPostCommand2, Hwnd, HwndInfo, HwndInfoType, MenuHookFlag,
    MidiInputDeviceId, MidiOutputDeviceId, ReaProject, ReaperStr, RegistrationHandle,
    SectionContext, ToolbarIconMap, WindowContext,
};
use reaper_rx::{ActionRxHookPostCommand, ActionRxHookPostCommand2};
use rxrust::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fs, mem};
use strum::IntoEnumIterator;
use swell_ui::{Menu, SharedView, View, ViewManager, Window};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use tracing::debug;
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
    BackboneShell::make_available_globally(|| {
        let backbone_shell = BackboneShell::init(context);
        register_plugin_destroy_hook(PluginDestroyHook {
            name: "BackboneShell",
            callback: || {
                let backbone_shell = BackboneShell::get();
                let _ = backbone_shell.go_to_sleep();
                backbone_shell.dispose();
            },
        });
        backbone_shell
    });
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
    /// This should always be set except in the destructor.
    ///
    /// The only reason why this is optional is that we need to take ownership of the runtime when the shell is
    /// dropped.
    async_runtime: RefCell<Option<Runtime>>,
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
    toolbar_change_detector: Option<RefCell<ToolbarChangeDetector>>,
    /// We need to keep this panel in memory in order to be informed when it's destroyed.
    _shutdown_detection_panel: SharedView<HiddenHelperPanel>,
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
        // Start async runtime
        // The main reason why we start it already here (and not wait until wake_up) is the async Sentry error reporting
        // initialization. However, more things might be added in the future. Plus, this runtime runs in its own thread,
        // so it doesn't clutter REAPER's main thread. It's fine if it runs in the background with nothing to do most
        // of the time.
        let async_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("Helgobox async runtime")
            .worker_threads(1)
            .build()
            .expect("couldn't start Helgobox async runtime");
        // Use "ring" as crypto provider (instead of "aws-lc-rs", because this is harder to build)
        let _ = rustls::crypto::ring::default_provider().install_default();
        // We need Swell already without VST plug-in instance to populate the extension menu. As soon as an instance
        // exists, we also need it for all the native GUI stuff.
        let _ = Swell::make_available_globally(Swell::load(context));
        // We need access to REAPER as soon as possible, of course
        // TODO-medium This needs around 10 MB of RAM. Of course only once, not per instance,
        //  so not a big deal. Still, maybe could be improved?
        let _ = Reaper::setup_with_defaults(context, create_plugin_info());
        // The API contains functions that must be around without any VST plug-in instance being active
        register_api().expect("couldn't register API");
        // Senders and receivers are initialized here but used only when awake. Yes, they already consume memory
        // when asleep but most of them are unbounded and therefore consume a minimal amount of memory as long as
        // they are not used.
        let config = BackboneConfig::load().unwrap_or_else(|e| {
            debug!("{}", e);
            Default::default()
        });
        // Init error reporting
        set_send_errors_to_dev_internal(config.send_errors_to_dev(), &async_runtime);
        set_show_errors_in_console_internal(config.show_errors_in_console());
        // Create channels
        let (control_surface_main_task_sender, control_surface_main_task_receiver) =
            SenderToNormalThread::new_unbounded_channel("control surface main tasks");
        let control_surface_main_task_sender =
            RealearnControlSurfaceMainTaskSender(control_surface_main_task_sender);
        #[cfg(feature = "playtime")]
        let (clip_matrix_event_sender, clip_matrix_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("Playtime matrix events");
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
        // even when no Helgobox instance exists anymore, because the HELGOBOX_LOG env variable is
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
            BackboneShell::helgoboss_resource_dir_path()
                .join("licensing.json")
                .into(),
            Box::new(BackboneLicenseManagerEventHandler),
        );
        // This just initializes the clip engine, it doesn't add any Playtime matrix yet, so resource consumption is low.
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
            BackboneShell::server_resource_dir_path()
                .join("certificates")
                .into(),
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
        let audio_hook = RealearnAudioHook::new(
            normal_audio_hook_task_receiver,
            feedback_audio_hook_task_receiver,
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
        let toolbar_change_detector = if custom_toolbar_api_is_available() {
            let observed_commands = ACTION_DEFS.iter().filter_map(|def| {
                if !def.add_toolbar_button {
                    return None;
                }
                // Auto-add previously enabled dynamic toolbar buttons, if not present already
                let enabled = config.toolbar.get(def.command_name).is_some_and(|v| *v > 0);
                if enabled {
                    let _ = add_or_remove_toolbar_button(def.command_name, true);
                }
                // Create change detector to automatically disable a dynamic toolbar button if the user removes the
                // button manually.
                let command_id = Reaper::get()
                    .action_by_command_name(def.command_name)
                    .command_id()
                    .ok()?;
                Some((command_id, def.command_name.to_string()))
            });
            let detector = ToolbarChangeDetector::new(observed_commands.collect());
            Some(RefCell::new(detector))
        } else {
            None
        };
        // Detect shutdown via hidden child window as suggested by Justin
        let shutdown_detection_panel = SharedView::new(HiddenHelperPanel::new());
        shutdown_detection_panel.clone().open(reaper_main_window());
        BackboneShell {
            async_runtime: RefCell::new(Some(async_runtime)),
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
            toolbar_change_detector,
            _shutdown_detection_panel: shutdown_detection_panel,
        }
    }

    /// Called when static is destroyed (REAPER exit or - on Windows if configured - complete VST
    /// unload if extension not loaded, just VST)
    pub fn dispose(&self) {
        println!("Disposing BackboneShell...");
        // Shutdown async runtime
        tracing::info!("Shutting down async runtime...");
        if let Ok(mut async_runtime) = self.async_runtime.try_borrow_mut() {
            if let Some(async_runtime) = async_runtime.take() {
                // 1 second timeout caused a freeze sometimes (Windows)
                async_runtime.shutdown_background();
            }
        }
        tracing::info!("Async runtime shut down successfully");
        let _ = Reaper::get().go_to_sleep();
        self.message_panel.close();
        let _ = unregister_api();
    }

    pub fn show_welcome_screen_if_necessary(&self) {
        {
            let mut config = self.config.borrow_mut();
            let showed_already = mem::replace(&mut config.main.showed_welcome_screen, 1) == 1;
            if showed_already {
                return;
            }
            notification::warn_user_on_anyhow_error(config.save());
        };
        Self::show_welcome_screen();
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

    /// Creates a new Helgobox instance on the given track.
    pub async fn create_new_instance_on_track(track: &Track) -> anyhow::Result<NewInstanceOutcome> {
        if Reaper::get().version().revision() < "6.69" {
            // Version too old to support TrackFX_AddByName with only VST2-UID specified
            bail!("Please update REAPER to the latest version to access this feature!");
        }
        let fx = track
            .normal_fx_chain()
            .add_fx_by_original_name(HELGOBOX_UNIQUE_VST_PLUGIN_ADD_STRING)
            .with_context(|| {
                if Reaper::get().vst_scan_is_enabled() {
                    "Looks like Helgobox VST plug-in is not installed! Did you follow the official installation instructions?"
                } else {
                    "It was not possible to add Helgobox VST plug-in, probably because you have VST scanning disabled in the REAPER preferences!\n\nPlease open REAPER's FX browser and press F5 to rescan. After that, try again!\n\nAs an alternative, re-enable VST scanning in Preferences/Plug-ins/VST and restart REAPER."
                }
            })?;
        fx.hide_floating_window()?;
        // The rest needs to be done async because the instance initializes itself async
        // (because FX not yet available when plug-in instantiated).
        millis(1).await;
        let instance_shell = Self::get().with_instance_shell_infos(
            |infos| -> anyhow::Result<SharedInstanceShell> {
                let last_info = infos.last().context("instance was not registered")?;
                let shared_instance = last_info
                    .instance_shell
                    .upgrade()
                    .context("instance gone")?;
                Ok(shared_instance)
            },
        )?;
        let outcome = NewInstanceOutcome { fx, instance_shell };
        Ok(outcome)
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
        // It's important to wait a bit, otherwise we risk the MIDI is not being sent.
        // We wait for 3 audio blocks, a maximum of 100 milliseconds. Justin's recommendation.
        let initial_block_count = GLOBAL_AUDIO_STATE.load_block_count();
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(1));
            let elapsed_blocks = GLOBAL_AUDIO_STATE
                .load_block_count()
                .saturating_sub(initial_block_count);
            if elapsed_blocks > 2 {
                debug!("Waited a total of {elapsed_blocks} blocks after sending shutdown MIDI messages");
                break;
            }
        }
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

    fn with_async_runtime<R>(&self, f: impl FnOnce(&Runtime) -> R) -> anyhow::Result<R> {
        let runtime = self
            .async_runtime
            .try_borrow()
            .context("async runtime already borrowed")?;
        let runtime = runtime
            .as_ref()
            .context("async runtime already destroyed")?;
        Ok(f(runtime))
    }

    /// Executed whenever the first Helgobox instance is loaded.
    ///
    /// This should fire up stuff that only needs to be around while awake (= as long as at least one Helgobox VST
    /// plug-in instance is around). Stuff that must be around even while asleep should be put into [Self::init].
    ///
    /// The opposite function is [Self::go_to_sleep].
    ///
    /// # Errors
    ///
    /// Returns and error if not sleeping.
    pub fn wake_up(&self) -> anyhow::Result<()> {
        let prev_state = self.state.replace(AppState::WakingUp);
        let AppState::Sleeping(mut sleeping_state) = prev_state else {
            self.state.replace(prev_state);
            bail!("App was not sleeping");
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
        // Activate server
        if self.config.borrow().server_is_enabled() {
            let _ = self.with_async_runtime(|runtime| {
                self.server()
                    .borrow_mut()
                    .start(runtime, self.create_services())
                    .unwrap_or_else(warn_about_failed_server_start);
            });
        }
        let mut session = Reaper::get().medium_session();
        // Action hooks
        session
            .plugin_register_add_hook_post_command::<ActionRxHookPostCommand<Global>>()
            .unwrap();
        session
            .plugin_register_add_hook_post_command::<Self>()
            .unwrap();
        // Window hooks (fails before REAPER 6.29)
        let _ = session.plugin_register_add_hwnd_info::<Self>();
        // This fails before REAPER 6.20 and therefore we don't have MIDI CC action feedback.
        let _ =
            session.plugin_register_add_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        let _ = session.plugin_register_add_hook_post_command_2::<Self>();
        // Audio hook
        debug!("Registering ReaLearn audio hook and control surface...");
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
        };
        self.state.replace(AppState::Awake(awake_state));
        Ok(())
    }

    // Executed whenever the last ReaLearn instance goes away.
    pub fn go_to_sleep(&self) -> anyhow::Result<()> {
        let prev_state = self.state.replace(AppState::GoingToSleep);
        let AppState::Awake(awake_state) = prev_state else {
            bail!("App was not awake when trying to go to sleep");
        };
        let mut session = Reaper::get().medium_session();
        debug!("Unregistering ReaLearn control surface and audio hook...");
        let (accelerator, mut control_surface, audio_hook) = unsafe {
            let accelerator = session
                .plugin_register_remove_accelerator(awake_state.accelerator_handle)
                .context("accelerator was not registered")?;
            let control_surface = session
                .plugin_register_remove_csurf_inst(awake_state.control_surface_handle)
                .context("control surface was not registered")?;
            let audio_hook = session
                .audio_reg_hardware_hook_remove(awake_state.audio_hook_handle)
                .context("control surface was not registered")?;
            (accelerator, control_surface, audio_hook)
        };
        // Close OSC connections
        let middleware = control_surface.middleware_mut();
        middleware.clear_osc_input_devices();
        self.osc_feedback_processor.borrow_mut().stop();
        // Window hooks
        session.plugin_register_remove_hwnd_info::<Self>();
        // Actions
        session.plugin_register_remove_hook_post_command_2::<Self>();
        session.plugin_register_remove_hook_post_command_2::<ActionRxHookPostCommand2<Global>>();
        session.plugin_register_remove_hook_post_command::<Self>();
        session.plugin_register_remove_hook_post_command::<ActionRxHookPostCommand<Global>>();
        // Server
        self.server().borrow_mut().stop();
        // Stop async deallocation thread
        GLOBAL_ALLOCATOR.stop_async_deallocation();
        let async_deallocation_receiver = awake_state
            .async_deallocation_thread
            .join()
            .map_err(|_| anyhow!("couldn't join deallocation thread"))?;
        // Finally go to sleep
        let sleeping_state = SleepingState {
            control_surface,
            audio_hook,
            accelerator,
            async_deallocation_receiver,
        };
        self.state.replace(AppState::Sleeping(sleeping_state));
        Ok(())
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
        let result = self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            // Remove main processor.
            control_surface
                .middleware_mut()
                .remove_main_processor(unit_id)
        });
        // Removing the main processor can fail if the removal of that ReaLearn instance was triggered by
        // ReaLearn itself (possibly by another instance) ... Sentry example ID: 0be87e420ee041538000e8628d941741.
        // Reentrancy! We can't simply leave it at that because this would mean that the main processor is still in the
        // list, so it would keep working without the plug-in being around anymore ... that would be super strange.
        // That's why we remove the main processor asynchronously as a fallback. A quick-and-dirty solution, yes.
        // TODO-medium This is ugly, to be honest. Ideally, we would have another solution:
        // a) A non-shared list of owned main processors + a shared list of weak main processors references.
        //    Then we could infallibly remove the owned main processors right here because we don't need to borrow.
        //    Code which uses the shared list of weak main processors can throw out the dead references at some point.
        //    Having the weak processor references in that list a bit longer doesn't hurt. They will just be skipped.
        // b) We would execute relevant targets (mainly "Project: Invoke REAPER action") asynchronously, that is, in
        //    the next main loop cycle. Then the main processors wouldn't be borrowed during the actual action
        //    execution. However, that would need a bit more testing.
        // c) Or always remove asynchronously. But that would also need some careful testing.
        if result.is_err() {
            self.control_surface_main_task_sender
                .remove_main_processor(unit_id);
        }
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

    fn temporarily_reclaim_control_surface_ownership<R>(
        &self,
        f: impl FnOnce(&mut RealearnControlSurface) -> R,
    ) -> R {
        let (result, next_state) = match self.state.replace(AppState::Suspended) {
            AppState::Sleeping(mut s) => {
                let result = f(&mut s.control_surface);
                (result, AppState::Sleeping(s))
            }
            AppState::Awake(s) => {
                let mut session = Reaper::get().medium_session();
                let mut control_surface = unsafe {
                    session
                        .plugin_register_remove_csurf_inst(s.control_surface_handle)
                        .expect("control surface was not registered")
                };
                // Execute necessary operations
                let result = f(&mut control_surface);
                // Give it back to REAPER.
                let control_surface_handle = session
                    .plugin_register_add_csurf_inst(control_surface)
                    .expect("couldn't reregister ReaLearn control surface");
                let awake_state = AwakeState {
                    control_surface_handle,
                    audio_hook_handle: s.audio_hook_handle,
                    accelerator_handle: s.accelerator_handle,
                    async_deallocation_thread: s.async_deallocation_thread,
                };
                (result, AppState::Awake(awake_state))
            }
            _ => panic!("Backbone was neither in sleeping nor in awake state"),
        };
        self.state.replace(next_state);
        result
    }

    /// Spawns the given future on the Helgobox async runtime.
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
            .expect("async runtime not available")
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
        let res = self.with_async_runtime(|runtime| {
            let start_result = self
                .server
                .borrow_mut()
                .start(runtime, self.create_services());
            self.change_config(BackboneConfig::enable_server);
            start_result
        });
        res.unwrap_or_else(|e| Err(e.to_string()))
    }

    pub fn stop_server_persistently(&self) {
        self.change_config(BackboneConfig::disable_server);
        self.server.borrow_mut().stop();
    }

    pub fn set_send_errors_to_dev_persistently(&self, value: bool) {
        // Persistence
        self.change_config(|c| c.set_send_errors_to_dev(value));
        // Actual behavior
        let _ = self.with_async_runtime(|runtime| {
            set_send_errors_to_dev_internal(value, runtime);
        });
    }

    pub fn set_show_errors_in_console_persistently(&self, value: bool) {
        // Persistence
        self.change_config(|c| c.set_show_errors_in_console(value));
        // Actual behavior
        set_show_errors_in_console_internal(value);
    }

    pub fn toggle_background_colors(&self) {
        self.change_config(BackboneConfig::toggle_background_colors);
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

    /// To be called regularly, maybe once a second.
    ///
    /// See https://github.com/helgoboss/helgobox/issues/1331.
    pub fn disable_manually_removed_dynamic_toolbar_buttons(&self) {
        let Some(detector) = &self.toolbar_change_detector else {
            return;
        };
        for command_name in detector.borrow_mut().detect_manually_removed_commands() {
            self.change_config(|config| {
                config.toolbar.insert(command_name.to_string(), 0);
            })
        }
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
        notification::warn_user_on_anyhow_error(config.save());
        self.notify_changed();
        result
    }

    fn helgoboss_resource_dir_path() -> Utf8PathBuf {
        Reaper::get().resource_path().join("Helgoboss")
    }

    pub fn app_dir_path() -> Utf8PathBuf {
        BackboneShell::helgoboss_resource_dir_path().join("App")
    }

    pub fn app_binary_base_dir_path() -> Utf8PathBuf {
        BackboneShell::app_dir_path().join("bin")
    }

    pub fn app_config_dir_path() -> Utf8PathBuf {
        BackboneShell::app_dir_path().join("etc")
    }

    pub fn app_settings_file_path() -> Utf8PathBuf {
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

    fn realearn_resource_dir_path() -> Utf8PathBuf {
        BackboneShell::helgoboss_resource_dir_path().join("ReaLearn")
    }

    pub fn helgobox_data_dir_path() -> Utf8PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/helgobox")
    }

    pub fn realearn_data_dir_path() -> Utf8PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/realearn")
    }

    pub fn app_archive_file_path() -> Utf8PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/archives/helgobox-app.tar.zst")
    }

    pub fn realearn_high_click_sound_path() -> Option<&'static Utf8Path> {
        static PATH: Lazy<Option<Utf8PathBuf>> = Lazy::new(|| {
            // Before including the audio file in the binary, there was an actual file distributed
            // via ReaPack. However, we had to copy it to a temporary directory anyway, otherwise
            // we would risk an error on Windows when attempting to install a new ReaLearn version
            // via ReaPack while still having ReaLearn open.
            // https://github.com/helgoboss/helgobox/issues/780
            // Encoding the file in the binary frees us from having to distribute it.
            let bytes = include_bytes!("../../../../resources/sounds/click-high.mp3");
            let dest_path = BackboneShell::get_temp_dir()?.path().join("click-high.mp3");
            fs::write(&dest_path, bytes).ok()?;
            dest_path.try_into().ok()
        });
        PATH.as_ref().map(|p| p.as_path())
    }

    #[cfg(feature = "egui")]
    pub fn realearn_pot_preview_template_path() -> Option<&'static Utf8Path> {
        static PATH: Lazy<Option<Utf8PathBuf>> = Lazy::new(|| {
            let bytes = include_bytes!(
                "../../../../resources/template-projects/pot-preview/pot-preview.RPP"
            );
            let dest_path = BackboneShell::get_temp_dir()?
                .path()
                .join("pot-preview.RPP");
            fs::write(&dest_path, bytes).ok()?;
            dest_path.try_into().ok()
        });
        PATH.as_ref().map(|p| p.as_path())
    }

    pub fn realearn_preset_dir_path() -> Utf8PathBuf {
        Self::realearn_data_dir_path().join("presets")
    }

    pub fn realearn_compartment_preset_dir_path(compartment: CompartmentKind) -> Utf8PathBuf {
        let sub_dir = match compartment {
            CompartmentKind::Controller => "controller",
            CompartmentKind::Main => "main",
        };
        Self::realearn_preset_dir_path().join(sub_dir)
    }

    pub fn realearn_auto_load_configs_dir_path() -> Utf8PathBuf {
        Self::realearn_data_dir_path().join("auto-load-configs")
    }

    pub fn realearn_osc_device_config_file_path() -> Utf8PathBuf {
        BackboneShell::realearn_resource_dir_path().join("osc.json")
    }

    pub fn realearn_controller_config_file_path() -> Utf8PathBuf {
        BackboneShell::realearn_resource_dir_path().join("controllers.json")
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

    pub fn register_instance(&self, instance_shell: &SharedInstanceShell) {
        debug!("Registering new instance...");
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
        self.proto_hub.notify_instances_changed();
    }

    pub fn unregister_instance(&self, instance_id: InstanceId) {
        debug!("Unregistering instance...");
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
        self.proto_hub.notify_instances_changed();
    }

    pub fn register_unit(
        &self,
        unit_info: UnitInfo,
        real_time_processor: SharedRealTimeProcessor,
        main_processor: MainProcessor<WeakUnitModel>,
    ) {
        let unit_id = unit_info.unit_id;
        debug!("Registering new unit {unit_id}...");
        let mut units = self.unit_infos.borrow_mut();
        if !unit_info.is_auto_unit {
            update_auto_units_async();
        }
        units.push(unit_info);
        debug!("Unit {unit_id} registered. Unit count: {}", units.len());
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
        debug!("Unregistering unit...");
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

    pub fn show_welcome_screen() {
        let shell = Self::get();
        open_child_panel(
            &shell.welcome_panel,
            WelcomePanel::new(),
            reaper_main_window(),
        );
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

    pub fn find_first_mapping_by_learnable_source() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .find_first_mapping_by_learnable_source_async(CompartmentKind::Main)
                .await;
            Ok(())
        });
    }

    pub fn learn_mapping_reassigning_learnable_source_open() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .learn_mapping_reassigning_learnable_source_async(CompartmentKind::Main, true)
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

    pub fn detect_controllers_with_logging() {
        spawn_in_main_thread(async {
            let reaper = Reaper::get();
            reaper.show_console_msg("== ReaLearn: Detecting controllers, please wait ...\n\n");
            let devices = Reaper::get()
                .midi_output_devices()
                .filter(|dev| dev.is_connected())
                .map(|dev| dev.id());
            let probe = controller_detection::detect_controllers(devices.collect())
                .await
                .inspect_err(notify_user_about_anyhow_error)?;
            for probe in probe {
                reaper.show_console_msg(probe.to_string())
            }
            reaper.show_console_msg("Controller detection finished.\n");
            Ok(())
        });
    }

    pub fn learn_mapping_reassigning_learnable_source() {
        Global::future_support().spawn_in_main_thread_from_main_thread(async {
            let _ = BackboneShell::get()
                .learn_mapping_reassigning_learnable_source_async(CompartmentKind::Main, false)
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
            playtime_impl::execute_playtime_show_hide_action(async {
                playtime_impl::show_or_hide_playtime(false).await
            });
        }
    }

    pub fn show_hide_custom_playtime() {
        #[cfg(feature = "playtime")]
        {
            playtime_impl::execute_playtime_show_hide_action(async {
                playtime_impl::show_or_hide_playtime(true).await
            });
        }
    }

    async fn find_first_mapping_by_learnable_source_async(
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
            let capture_result = tokio::select! {
                Ok(r) = midi_receiver.recv() => {
                    Some(MessageCaptureResult::Midi(r))
                }
                Ok(r) = osc_receiver.recv() => {
                    Some(MessageCaptureResult::Osc(r))
                }
                else => None
            };
            let Some(r) = capture_result else {
                break;
            };
            if let Some(outcome) =
                self.find_first_relevant_session_with_source_matching(compartment, &r)
            {
                if outcome.source_is_learnable {
                    self.close_message_panel();
                    outcome
                        .unit_model
                        .borrow()
                        .show_mapping(compartment, outcome.mapping.borrow().id());
                }
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

    async fn learn_mapping_reassigning_learnable_source_async(
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
        self.show_message_panel("ReaLearn", "Touch a control element!", || {
            BackboneShell::stop_learning_sources();
        });
        let midi_receiver = self.request_next_midi_messages();
        let osc_receiver = self.request_next_osc_messages();
        // Try until we found the first learnable message
        let (unit_model, existing_mapping, capture_result) = loop {
            // Capture next result
            let capture_result = tokio::select! {
                Ok(r) = midi_receiver.recv() => {
                    Some(MessageCaptureResult::Midi(r))
                }
                Ok(r) = osc_receiver.recv() => {
                    Some(MessageCaptureResult::Osc(r))
                }
                else => None
            };
            let Some(capture_result) = capture_result else {
                return Ok(());
            };
            // Make sure that there's at least one unit which has that control input
            let unit_model = if let Some(s) = capture_result
                .to_input_descriptor(false)
                .and_then(|id| self.find_first_relevant_session_with_input_from(&id))
            {
                s
            } else {
                self.close_message_panel_with_alert(
                    "No ReaLearn unit found which has this control input! First please add one to the monitoring FX chain or this project and set the MIDI control input accordingly!",
                );
                return Err("no ReaLearn unit with that input");
            };
            // Find mapping that matches the source
            if let Some(outcome) =
                self.find_first_relevant_session_with_source_matching(compartment, &capture_result)
            {
                // We found a mapping!
                if outcome.source_is_learnable {
                    // Yo, we found a matching mapping. Use that one for the rest of the procedure.
                    break (outcome.unit_model, Some(outcome.mapping), capture_result);
                } else {
                    // Wait for next message because this source is not learnable
                    continue;
                }
            } else {
                // Couldn't find that one. Check if mapping is learnable in the unit that we found.
                let virtualization = unit_model
                    .borrow()
                    .virtualize_source_value(capture_result.message());
                if let Some(v) = virtualization {
                    if !v.learnable {
                        // Ignore because this source is not learnable in that unit model.
                        continue;
                    }
                }
                // We will have to create a new mapping.
                break (unit_model, None, capture_result);
            };
        };
        // Now learn target
        let reaper_target = self
            .prompt_for_next_reaper_target("Now touch the desired target!")
            .await?;
        // Close panel
        self.close_message_panel();
        // Modify existing mapping or add new mapping
        let mapping = if let Some(mapping) = existing_mapping {
            // There's already a mapping with that source. Change target of that mapping.
            {
                let mut m = mapping.borrow_mut();
                unit_model.borrow_mut().change_target_with_closure(
                    &mut m,
                    None,
                    Rc::downgrade(&unit_model),
                    |ctx| {
                        ctx.mapping.target_model.apply_from_target(
                            &reaper_target,
                            ctx.extended_context,
                            compartment,
                        )
                    },
                );
            }
            mapping
        } else {
            // There's no mapping with that source yet. Add it to the previously determined first
            // session.
            let mapping = {
                let mut s = unit_model.borrow_mut();
                let mapping = s.add_default_mapping(
                    compartment,
                    GroupId::default(),
                    VirtualControlElementCharacter::Multi,
                );
                let mut m = mapping.borrow_mut();
                let event = MessageCaptureEvent {
                    result: capture_result,
                    allow_virtual_sources: true,
                    osc_arg_index_hint: None,
                };
                let compound_source = s
                    .create_compound_source_for_learning(event)
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
            mapping
        };
        if open_mapping {
            unit_model
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
    ) -> anyhow::Result<RequestMidiDeviceIdentityReply> {
        let reply_receiver =
            self.request_midi_device_identity_internal(output_device_id, input_device_id);
        reply_receiver
            .recv()
            .await
            .map_err(|_| anyhow!("no MIDI device identity reply received"))
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
        let in_current_project = self.find_first_session_with_target(
            Some(Reaper::get().current_project()),
            compartment,
            target,
        );
        in_current_project
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
        let in_current_project = self.find_first_session_with_input_from(
            Some(Reaper::get().current_project()),
            input_descriptor,
        );
        in_current_project
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
    ) -> Option<MatchingSourceOutcome> {
        let in_current_project = self.find_first_session_with_source_matching(
            Some(Reaper::get().current_project()),
            compartment,
            capture_result,
        );
        in_current_project.or_else(|| {
            self.find_first_session_with_source_matching(None, compartment, capture_result)
        })
    }

    fn find_first_session_with_source_matching(
        &self,
        project: Option<Project>,
        compartment: CompartmentKind,
        capture_result: &MessageCaptureResult,
    ) -> Option<MatchingSourceOutcome> {
        self.unit_infos.borrow().iter().find_map(|session| {
            let unit_model = session.unit_model.upgrade()?;
            let outcome = {
                let s = unit_model.borrow();
                if s.processor_context().project() != project {
                    return None;
                }
                let input_descriptor = capture_result.to_input_descriptor(true)?;
                if !s.receives_input_from(&input_descriptor) {
                    return None;
                }
                s.find_mapping_with_source(compartment, capture_result.message())?
            };
            let outcome = MatchingSourceOutcome {
                unit_model,
                mapping: outcome.mapping,
                source_is_learnable: outcome.source_is_learnable,
            };
            Some(outcome)
        })
    }

    fn server_resource_dir_path() -> Utf8PathBuf {
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
    // Map from command name (e.g. HB_SHOW_HIDE_PLAYTIME to state integer, 0 for disabled , 1 for enabled)
    toolbar: HashMap<String, u8>,
}

impl BackboneConfig {
    pub fn load() -> anyhow::Result<BackboneConfig> {
        let ini_content =
            fs::read_to_string(Self::config_file_path()).context("couldn't read config file")?;
        let config = serde_ini::from_str(&ini_content)?;
        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let ini_content = serde_ini::to_string(self).context("couldn't serialize config")?;
        let config_file = Self::config_file_path();
        let config_file_dir = config_file.parent().unwrap();
        fs::create_dir_all(config_file_dir).with_context(|| {
            format!(
                "Couldn't create configuration directory '{config_file_dir}'. Make sure that REAPER has write access to this directory!"
            )
        })?;
        fs::write(&config_file, ini_content)
            .with_context(|| format!("Couldn't write config file '{config_file}'. Make sure that REAPER has write access to this file!"))?;
        Ok(())
    }

    pub fn send_errors_to_dev(&self) -> bool {
        self.main.send_errors_to_dev > 0
    }

    pub fn set_send_errors_to_dev(&mut self, value: bool) {
        self.main.send_errors_to_dev = value.into();
    }

    pub fn show_errors_in_console(&self) -> bool {
        self.main.show_errors_in_console > 0
    }

    pub fn set_show_errors_in_console(&mut self, value: bool) {
        self.main.show_errors_in_console = value.into();
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

    pub fn background_colors_enabled(&self) -> bool {
        self.main.background_colors_enabled > 0
    }

    pub fn toggle_background_colors(&mut self) {
        self.main.background_colors_enabled = if self.background_colors_enabled() {
            0
        } else {
            1
        };
    }

    pub fn toolbar_button_is_enabled(&self, command_name: &str) -> bool {
        self.toolbar.get(command_name).is_some_and(|v| *v != 0)
    }

    fn config_file_path() -> Utf8PathBuf {
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
    #[serde(
        default = "default_background_colors_enabled",
        skip_serializing_if = "is_default_background_colors_enabled"
    )]
    background_colors_enabled: u8,
    #[serde(
        default = "default_send_errors_to_dev",
        skip_serializing_if = "is_default_send_errors_to_dev"
    )]
    send_errors_to_dev: u8,
    #[serde(
        default = "default_show_errors_in_console",
        skip_serializing_if = "is_default_show_errors_in_console"
    )]
    show_errors_in_console: u8,
}

const DEFAULT_SERVER_HTTP_PORT: u16 = 39080;
const DEFAULT_SERVER_HTTPS_PORT: u16 = 39443;
const DEFAULT_SERVER_GRPC_PORT: u16 = 39051;
const DEFAULT_BACKGROUND_COLORS_ENABLED: u8 = 1;
const DEFAULT_SHOW_ERRORS_IN_CONSOLE: u8 = 1;
/// For existing installations, we don't enable this (would change behavior).
const DEFAULT_SEND_ERRORS_TO_DEV: u8 = 0;

fn default_background_colors_enabled() -> u8 {
    DEFAULT_BACKGROUND_COLORS_ENABLED
}

fn is_default_background_colors_enabled(v: &u8) -> bool {
    *v == DEFAULT_BACKGROUND_COLORS_ENABLED
}

fn default_show_errors_in_console() -> u8 {
    DEFAULT_SHOW_ERRORS_IN_CONSOLE
}

fn is_default_show_errors_in_console(v: &u8) -> bool {
    *v == DEFAULT_SHOW_ERRORS_IN_CONSOLE
}

fn default_send_errors_to_dev() -> u8 {
    DEFAULT_SEND_ERRORS_TO_DEV
}

fn is_default_send_errors_to_dev(v: &u8) -> bool {
    *v == DEFAULT_SEND_ERRORS_TO_DEV
}

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
        // This default implementation is used when Helgobox hasn't been used before. It's the initial config.
        MainConfig {
            server_enabled: 0,
            server_http_port: default_server_http_port(),
            server_https_port: default_server_https_port(),
            server_grpc_port: default_server_grpc_port(),
            companion_web_app_url: default_companion_web_app_url(),
            showed_welcome_screen: 0,
            background_colors_enabled: default_background_colors_enabled(),
            show_errors_in_console: 1,
            // For new installations, this is an opt-out. The welcome screen will be shown for sure, so that's okay.
            send_errors_to_dev: 1,
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

impl HwndInfo for BackboneShell {
    fn call(hwnd: Option<Hwnd>, info_type: HwndInfoType, msg: Option<AccelMsg>) -> i32 {
        let Some(hwnd) = hwnd else {
            return 0;
        };
        const IGNORE: i32 = 0;
        const PASS_TO_WINDOW: i32 = 1;
        const PROCESS_GLOBALLY: i32 = -1;
        // Special handling for SPACE key: Always let it process by Helgobox App even if defined as Global.
        // Without that, users who defined SPACE as global hotkey wouldn't be able to enjoy the special SPACE key
        // behavior in Playtime (playing Playtime only without REAPER when App window is focused).
        if let Some(msg) = msg {
            if msg.key().get() as u32 == raw::VK_SPACE && is_app_window(hwnd) {
                debug!("Pressed global space in app window");
                return PASS_TO_WINDOW;
            }
        }
        // Continue if no special SPACE handling invoked
        match info_type {
            HwndInfoType::IsTextField => {
                let window = Window::from_hwnd(hwnd);
                // Check if egui App window is in text entry mode.
                if let Some(realearn_view) =
                    BackboneHelgoboxWindowSnitch.find_closest_realearn_view(window)
                {
                    if realearn_view.wants_raw_keyboard_input() {
                        // We are in an egui app. Let's just always assume it's in text entry mode. In practice,
                        // this is mostly the case. https://github.com/helgoboss/helgobox/issues/1288
                        return PASS_TO_WINDOW;
                    }
                }
                // Check if Helgobox App window is in text entry mode.
                // This one is only necessary on Windows, see https://github.com/helgoboss/helgobox/issues/1083.
                // REAPER detected a global hotkey press while having a child window focused. It wants to know whether
                // this child window is currently in text-entry mode, in which case it would NOT execute the action
                // associated with the global hotkey but direct the key to the window. We must check here whether
                // the Helgobox App is currently in text entry mode. This is only necessary on Windows, because
                // Flutter essentially just uses one big HWND on windows ... text fields are not different HWNDs and
                // therefore not identifiable as text field (via Window classes "Edit", "RichEdit" etc.).
                // When we end up here, we are on Windows (for macOS, the hook is not registered).
                // The queried window has a parent
                if !cfg!(windows) {
                    return IGNORE;
                }
                let Some(parent_window) = window.parent() else {
                    return IGNORE;
                };
                match app_window_is_in_text_entry_mode(parent_window.raw_hwnd()) {
                    None => {
                        // Probably not a Helgobox App window
                        IGNORE
                    }
                    Some(false) => {
                        // It's a Helgobox App window, but we are not in text entry mode
                        PROCESS_GLOBALLY
                    }
                    Some(true) => {
                        // It's a Helgobox App, and we are in text entry mode
                        PASS_TO_WINDOW
                    }
                }
            }
            HwndInfoType::ShouldProcessGlobalHotkeys | HwndInfoType::Unknown(_) => {
                // This is called when the hotkey is defined with scope "Global + text fields". In this case,
                // we don't need to do anything because we want the global hotkey to fire.
                0
            }
        }
    }
}

impl HookPostCommand2 for BackboneShell {
    fn call(
        section: SectionContext,
        command_id: CommandId,
        _value_change: ActionValueChange,
        window: WindowContext,
        project: ReaProject,
    ) {
        if section == SectionContext::MainSection {
            // Process executed main action as feedback
            BackboneShell::get()
                .additional_feedback_event_sender
                .send_complaining(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                    section_context: SectionContext::MainSection,
                    command_id,
                }));
        }
        #[cfg(not(feature = "playtime"))]
        {
            let _ = (window, project);
        }
        #[cfg(feature = "playtime")]
        {
            post_process_action_invocation_for_playtime(section, command_id, window, project);
        }
    }
}

#[cfg(feature = "playtime")]
fn post_process_action_invocation_for_playtime(
    context: SectionContext,
    command_id: CommandId,
    window: WindowContext,
    project: ReaProject,
) {
    match context {
        SectionContext::MainSection => {
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
        SectionContext::Sec(sec) => {
            let WindowContext::Win(hwnd) = window else {
                return;
            };
            if sec.unique_id().get() == 32060 {
                // MIDI editor section
                let play_stop_command_id = CommandId::new(40016);
                if command_id == play_stop_command_id {
                    // Playback within a MIDI editor has been started or stopped. If this is a MIDI editor for a
                    // Playtime clip, we should stop playback of that clip. In both cases.
                    BackboneShell::get().with_instance_shell_infos(|infos| {
                        let relevant_helgobox_instances = infos
                            .iter()
                            .filter(|info| {
                                info.processor_context.project_or_current_project().raw() == project
                            })
                            .flat_map(|info| info.instance.upgrade());
                        for instance in relevant_helgobox_instances {
                            let mut instance = instance.borrow_mut();
                            if let Some(matrix) = instance.clip_matrix_mut() {
                                matrix.panic_slot_with_open_midi_editor(hwnd);
                            }
                        }
                    });
                }
            }
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
                    let _ = fx.enable();
                } else {
                    let _ = fx.disable();
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
        let added_devices = diff.added_devices.iter().copied().collect();
        spawn_in_main_thread(async move {
            let _ = controller_detection::detect_controllers(added_devices).await;
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
            use helgobox_api::runtime::GlobalInfoEvent;
            // Let the Playtime Clip Engine check if it finds a suitable license
            let success = playtime_clip_engine::PlaytimeEngine::get()
                .main()
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
    let lib = crate::infrastructure::ui::AppLibrary::load(app_base_dir.into());
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
        Reaper::get().with_our_command(command_id, |command| {
            let command_name = command?.command_name();
            let action_def = ACTION_DEFS
                .iter()
                .find(|d| d.command_name == command_name)?;
            action_def
                .icon
                .map(|icon| unsafe { ReaperStr::from_ptr(icon.as_ptr()) })
        })
    }
}

pub fn reaper_main_window() -> Window {
    Window::from_hwnd(Reaper::get().main_window())
}

pub struct NewInstanceOutcome {
    pub fx: Fx,
    pub instance_shell: SharedInstanceShell,
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::base::notification;
    use crate::domain::err_if_reaper_version_too_low_for_playtime;
    use crate::infrastructure::data::LicenseManager;
    use crate::infrastructure::plugin::{BackboneShell, NewInstanceOutcome};
    use crate::infrastructure::ui::util::open_in_browser;
    use crate::infrastructure::ui::AppPage;
    use anyhow::Context;
    use base::future_util::millis;
    use base::metrics_util::{record_duration, record_occurrence};
    use base::spawn_in_main_thread;
    use camino::Utf8PathBuf;
    use futures::future::BoxFuture;
    use playtime_api::persistence::PlaytimeSettings;
    use playtime_clip_engine::PlaytimeEngine;
    use reaper_high::{GroupingBehavior, Project, Reaper};
    use reaper_medium::{
        GangBehavior, InputMonitoringMode, MessageBoxResult, MessageBoxType, OpenProjectBehavior,
        RecordingInput,
    };
    use std::fs;
    use std::future::Future;

    impl BackboneShell {
        pub(crate) fn read_playtime_settings() -> Option<PlaytimeSettings> {
            let json = fs::read_to_string(Self::playtime_settings_file_path()).ok()?;
            serde_json::from_str(&json).ok()
        }

        pub fn playtime_settings_file_path() -> Utf8PathBuf {
            Self::playtime_dir_path().join("settings.json")
        }

        pub fn playtime_dir_path() -> Utf8PathBuf {
            Self::helgoboss_resource_dir_path().join("Playtime")
        }
    }

    pub fn execute_playtime_show_hide_action(
        future: impl Future<Output = anyhow::Result<()>> + 'static,
    ) {
        // It's possible that the backbone shell is not yet woken up at this point, in which case
        // spawning wouldn't work.
        let _ = BackboneShell::get().wake_up();
        spawn_in_main_thread(async {
            let result = future.await;
            notification::notify_user_on_anyhow_error(result);
            Ok(())
        })
    }
    /// Creates a new track in the given project and adds a new Helgobox instance to it.
    pub async fn create_new_instance_in_project(
        project: Project,
        track_name: &str,
    ) -> anyhow::Result<NewInstanceOutcome> {
        let track = project.insert_track_at(0)?;
        track.set_name(track_name);
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
        BackboneShell::create_new_instance_on_track(&track).await
    }

    pub fn init_clip_engine(license_manager: &LicenseManager) {
        #[derive(Debug)]
        struct RealearnMetricsRecorder;
        impl playtime_clip_engine::MetricsRecorder for RealearnMetricsRecorder {
            fn record_duration(&self, id: &'static str, delta: std::time::Duration) {
                record_duration(id, delta);
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
        struct HelgoboxPlaytimeIntegration;
        impl playtime_clip_engine::PlaytimeIntegration for HelgoboxPlaytimeIntegration {
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

            fn spawn_in_async_runtime(&self, f: BoxFuture<'static, ()>) {
                BackboneShell::get().spawn_in_async_runtime(f);
            }
        }
        let args = playtime_clip_engine::PlaytimeEngineInitArgs {
            available_licenses: license_manager.licenses(),
            settings: BackboneShell::read_playtime_settings(),
            metrics_recorder,
            integration: Box::new(HelgoboxPlaytimeIntegration),
        };
        PlaytimeEngine::make_available_globally(PlaytimeEngine::new(args));
    }

    fn enable_playtime_for_first_helgobox_instance_and_show_it() -> anyhow::Result<()> {
        // let plugin_context = Reaper::get().medium_reaper().low().plugin_context();
        // We don't really need to do that via the external API but on the other hand, this is the only
        // example so far where we actually use our exposed API! If we don't "eat our own dogfood", we would have
        // to add integration tests in order to quickly realize if this works or not.
        // TODO-low Add integration tests instead of using API here.
        // let helgobox_api = helgobox_api::runtime::HelgoboxApiSession::load(plugin_context)
        //     .context("Couldn't load Helgobox API even after adding Helgobox. Old version?")?;
        // let playtime_api = playtime_api::runtime::PlaytimeApiSession::load(plugin_context)
        //     .context("Couldn't load Playtime API even after adding Helgobox. Old version? Or Helgobox built without Playtime?")?;
        // let instance_id = helgobox_api.HB_FindFirstHelgoboxInstanceInProject(std::ptr::null_mut());
        // playtime_api.HB_CreateClipMatrix(instance_id);
        // playtime_api.HB_ShowOrHidePlaytime(instance_id);
        let project = Reaper::get().current_project();
        let instance_id = BackboneShell::get()
            .find_first_helgobox_instance_in_project(project)
            .context("Couldn't find any Helgobox instance in this project.")?;
        let instance_shell = BackboneShell::get()
            .find_instance_shell_by_instance_id(instance_id)
            .context("instance not found")?;
        instance_shell
            .clone()
            .insert_owned_clip_matrix_if_necessary()?;
        instance_shell
            .panel()
            .start_show_or_hide_app_instance(Some(AppPage::Playtime));
        Ok(())
    }

    pub async fn show_or_hide_playtime(use_template: bool) -> anyhow::Result<()> {
        let project = Reaper::get().current_project();
        match BackboneShell::get().find_first_playtime_helgobox_instance_in_project(project) {
            None => {
                // Project doesn't have any Playtime-enabled Helgobox instance yet. Add one.
                err_if_reaper_version_too_low_for_playtime()?;
                if use_template {
                    let resource_path = Reaper::get().resource_path();
                    let template_path =
                        resource_path.join("TrackTemplates/Playtime.RTrackTemplate");
                    if !template_path.exists() {
                        let msg = "Before using this function, please create a custom top-level REAPER track template called \"Playtime\"!\n\nIt should include your customized Playtime track as well as any used column tracks.\n\nDo you need further help?";
                        let result = Reaper::get().medium_reaper().show_message_box(
                            msg,
                            "Playtime",
                            MessageBoxType::YesNo,
                        );
                        if result == MessageBoxResult::Yes {
                            open_in_browser(
                                "https://docs.helgoboss.org/playtime/goto#start-custom-playtime",
                            );
                        }
                        return Ok(());
                    }
                    Reaper::get()
                        .medium_reaper()
                        .main_open_project(&template_path, OpenProjectBehavior::default());
                    millis(100).await;
                    let instance_id = BackboneShell::get()
                        .find_first_helgobox_instance_in_project(project)
                        .context("Looks like your track template \"Playtime\" doesn't contain any Helgobox instance. Please recreate the Playtime track template and try again!")?;
                    let instance_shell = BackboneShell::get()
                        .find_instance_shell_by_instance_id(instance_id)
                        .context("instance not found")?;
                    instance_shell
                        .panel()
                        .start_show_or_hide_app_instance(Some(AppPage::Playtime));
                } else {
                    create_new_instance_in_project(project, "Playtime").await?;
                    enable_playtime_for_first_helgobox_instance_and_show_it()?;
                }
            }
            Some(instance_id) => {
                let instance_panel = BackboneShell::get()
                    .find_instance_panel_by_instance_id(instance_id)
                    .context("Instance not found")?;
                instance_panel.start_show_or_hide_app_instance(Some(AppPage::Playtime));
            }
        }
        Ok(())
    }
}

struct MatchingSourceOutcome {
    unit_model: SharedUnitModel,
    mapping: SharedMapping,
    source_is_learnable: bool,
}

fn create_plugin_info() -> PluginInfo {
    PluginInfo {
        plugin_name: "Helgobox".to_string(),
        plugin_version: built_info::PKG_VERSION.to_string(),
        plugin_version_long: BackboneShell::detailed_version_label().to_string(),
        support_email_address: "info@helgoboss.org".to_string(),
        update_url: "https://www.helgoboss.org/projects/helgobox".to_string(),
    }
}

fn set_send_errors_to_dev_internal(value: bool, async_runtime: &Runtime) {
    Reaper::get().set_report_crashes_to_sentry(value);
    // Activate sentry if necessary
    if value {
        async_runtime.spawn(sentry::init_sentry(create_plugin_info()));
    }
}

fn set_show_errors_in_console_internal(value: bool) {
    Reaper::get().set_log_crashes_to_console(value);
}
