use crate::application::{
    RealearnControlSurfaceMainTaskSender, Session, SessionCommand, SharedMapping, SharedSession,
    VirtualControlElementType, WeakSession,
};
use crate::base::notification;
use crate::domain::{
    ActionInvokedEvent, AdditionalFeedbackEvent, BackboneState, ChangeInstanceFxArgs,
    ChangeInstanceTrackArgs, Compartment, EnableInstancesArgs, Exclusivity, FeedbackAudioHookTask,
    GroupId, InputDescriptor, InstanceContainer, InstanceContainerCommonArgs,
    InstanceFxChangeRequest, InstanceId, InstanceOrchestrationEvent, InstanceTrackChangeRequest,
    LastTouchedTargetFilter, MainProcessor, MessageCaptureEvent, MessageCaptureResult,
    MidiScanResult, NormalAudioHookTask, OscDeviceId, OscFeedbackProcessor, OscFeedbackTask,
    OscScanResult, ProcessorContext, QualifiedMappingId, RealearnAccelerator, RealearnAudioHook,
    RealearnControlSurfaceMainTask, RealearnControlSurfaceMiddleware, RealearnTarget,
    RealearnTargetState, RealearnWindowSnitch, ReaperTarget, ReaperTargetType,
    SharedMainProcessors, SharedRealTimeProcessor, Tag, WeakInstanceState,
};
use crate::infrastructure::data::{
    ExtendedPresetManager, FileBasedControllerPresetManager, FileBasedMainPresetManager,
    FileBasedPresetLinkManager, OscDevice, OscDeviceManager, SharedControllerPresetManager,
    SharedMainPresetManager, SharedOscDeviceManager, SharedPresetLinkManager,
};
use crate::infrastructure::server;
use crate::infrastructure::server::{
    MetricsReporter, RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL,
};
use crate::infrastructure::ui::{MainPanel, MessagePanel};
use base::default_util::is_default;
use base::{
    make_available_globally_in_main_thread_on_demand, Global, NamedChannelSender,
    SenderToNormalThread, SenderToRealTimeThread,
};
use enum_iterator::IntoEnumIterator;

use crate::base::allocator::{RealearnAllocatorIntegration, RealearnDeallocator, GLOBAL_ALLOCATOR};
use crate::infrastructure::plugin::api_impl::{register_api, unregister_api};
use crate::infrastructure::plugin::debug_util::resolve_symbols_from_clipboard;
use crate::infrastructure::plugin::tracing_util::TracingHook;
use crate::infrastructure::server::services::RealearnServices;
use crate::infrastructure::test::run_test;
use anyhow::{bail, Context};
use base::metrics_util::MetricsHook;
use helgoboss_allocator::{start_async_deallocation_thread, AsyncDeallocatorCommandReceiver};
use once_cell::sync::Lazy;
use realearn_api::persistence::{
    Envelope, FxChainDescriptor, FxDescriptor, TargetTouchCause, TrackDescriptor, TrackFxChain,
};
use reaper_high::{
    ActionKind, CrashInfo, Fx, Guid, MiddlewareControlSurface, Project, Reaper, Track,
};
use reaper_low::{PluginContext, Swell};
use reaper_medium::{
    AcceleratorPosition, ActionValueChange, CommandId, HookPostCommand, HookPostCommand2,
    ReaProject, RegistrationHandle, SectionContext, WindowContext,
};
use reaper_rx::{ActionRxHookPostCommand, ActionRxHookPostCommand2};
use rxrust::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use slog::{debug, Drain};
use std::cell::{Ref, RefCell};
use std::collections::HashSet;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};
use swell_ui::{SharedView, View, ViewManager, Window};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use url::Url;

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

make_available_globally_in_main_thread_on_demand!(App);

#[cfg(feature = "playtime")]
static APP_LIBRARY: std::sync::OnceLock<anyhow::Result<crate::infrastructure::ui::AppLibrary>> =
    std::sync::OnceLock::new();

pub type RealearnSessionAccelerator = RealearnAccelerator<WeakSession, RealearnSnitch>;

pub type RealearnControlSurface =
    MiddlewareControlSurface<RealearnControlSurfaceMiddleware<WeakSession>>;

#[derive(Debug)]
pub struct App {
    /// RAII
    _tracing_hook: Option<TracingHook>,
    /// RAII
    _metrics_hook: Option<MetricsHook>,
    state: RefCell<AppState>,
    controller_preset_manager: SharedControllerPresetManager,
    main_preset_manager: SharedMainPresetManager,
    preset_link_manager: SharedPresetLinkManager,
    osc_device_manager: SharedOscDeviceManager,
    server: SharedRealearnServer,
    config: RefCell<AppConfig>,
    sessions_changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    control_surface_main_task_sender: RealearnControlSurfaceMainTaskSender,
    #[cfg(feature = "playtime")]
    clip_matrix_event_sender: SenderToNormalThread<crate::domain::QualifiedClipMatrixEvent>,
    osc_feedback_task_sender: SenderToNormalThread<OscFeedbackTask>,
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    feedback_audio_hook_task_sender: SenderToRealTimeThread<FeedbackAudioHookTask>,
    instance_orchestration_event_sender: SenderToNormalThread<InstanceOrchestrationEvent>,
    audio_hook_task_sender: SenderToRealTimeThread<NormalAudioHookTask>,
    instances: RefCell<Vec<PluginInstanceInfo>>,
    instances_changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    message_panel: SharedView<MessagePanel>,
    osc_feedback_processor: Rc<RefCell<OscFeedbackProcessor>>,
    #[cfg(feature = "playtime")]
    clip_engine_hub: playtime_clip_engine::proto::ClipEngineHub,
}

#[derive(Debug)]
pub struct PluginInstanceInfo {
    pub instance_id: InstanceId,
    pub processor_context: ProcessorContext,
    pub instance_state: WeakInstanceState,
    pub session: WeakSession,
    pub ui: Weak<MainPanel>,
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

impl App {
    /// Executed globally just once when module loaded.
    pub fn init(context: PluginContext) -> Self {
        let logger = App::logger().clone();
        Swell::make_available_globally(Swell::load(context));
        // TODO-medium This needs around 10 MB of RAM. Of course only once, not per instance,
        //  so not a big deal. Still, maybe could be improved?
        Reaper::setup_with_defaults(
            context,
            logger,
            CrashInfo {
                plugin_name: "ReaLearn".to_string(),
                plugin_version: App::detailed_version_label().to_string(),
                support_email_address: "info@helgoboss.org".to_string(),
            },
        );
        register_api().expect("couldn't register API");
        let config = AppConfig::load().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
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
        // threads. However, this thread will only exist in awaken state.
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
        #[cfg(feature = "playtime")]
        Self::init_clip_engine();
        let backbone_state = BackboneState::new(
            additional_feedback_event_sender.clone(),
            RealearnTargetState::new(additional_feedback_event_sender.clone()),
        );
        BackboneState::make_available_globally(|| backbone_state);
        let sessions_changed_subject: RefCell<LocalSubject<'static, (), ()>> = Default::default();
        server::http::keep_informing_clients_about_sessions(
            sessions_changed_subject.borrow().clone(),
        );
        let controller_preset_manager = FileBasedControllerPresetManager::new(
            App::realearn_preset_dir_path().join("controller"),
        );
        let main_preset_manager =
            FileBasedMainPresetManager::new(App::realearn_preset_dir_path().join("main"));
        let preset_link_manager =
            FileBasedPresetLinkManager::new(App::realearn_auto_load_configs_dir_path());
        // This doesn't yet start listening for OSC messages (will happen on wake up)
        let osc_device_manager = OscDeviceManager::new(App::realearn_osc_device_config_file_path());
        // This doesn't yet start the server (will happen on wake up)
        let server = RealearnServer::new(
            config.main.server_http_port,
            config.main.server_https_port,
            config.main.server_grpc_port,
            App::server_resource_dir_path().join("certificates"),
            MetricsReporter::new(),
        );
        let osc_feedback_processor = OscFeedbackProcessor::new(osc_feedback_task_receiver);
        osc_device_manager
            .changed()
            .subscribe(|_| App::get().reconnect_osc_devices());
        let shared_main_processors = SharedMainProcessors::default();
        // This doesn't yet activate the control surface (will happen on wake up)
        let control_surface = MiddlewareControlSurface::new(RealearnControlSurfaceMiddleware::new(
            App::logger(),
            control_surface_main_task_receiver,
            #[cfg(feature = "playtime")]
            clip_matrix_event_receiver,
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            shared_main_processors.clone(),
        ));
        // This doesn't yet activate the audio hook (will happen on wake up)
        let audio_hook = RealearnAudioHook::new(
            normal_audio_hook_task_receiver,
            feedback_audio_hook_task_receiver,
        );
        // This doesn't yet activate the accelerator (will happen on wake up)
        let accelerator = RealearnAccelerator::new(shared_main_processors, RealearnSnitch);
        // Silently decompress app and load library in background so it's ready when needed
        #[cfg(feature = "playtime")]
        let _ = std::thread::Builder::new()
            .name("Helgobox app loader".to_string())
            .spawn(|| {
                let result = decompress_app().and_then(|_| load_app_library());
                let _ = APP_LIBRARY.set(result);
            });
        // REAPER registers/unregisters actions automatically depending on presence of plug-in
        Self::register_actions(&control_surface_main_task_sender);
        let sleeping_state = SleepingState {
            control_surface: Box::new(control_surface),
            audio_hook: Box::new(audio_hook),
            accelerator: Box::new(accelerator),
            async_deallocation_receiver,
        };
        App {
            _tracing_hook: tracing_hook,
            _metrics_hook: metrics_hook,
            state: RefCell::new(AppState::Sleeping(sleeping_state)),
            controller_preset_manager: Rc::new(RefCell::new(controller_preset_manager)),
            main_preset_manager: Rc::new(RefCell::new(main_preset_manager)),
            preset_link_manager: Rc::new(RefCell::new(preset_link_manager)),
            osc_device_manager: Rc::new(RefCell::new(osc_device_manager)),
            server: Rc::new(RefCell::new(server)),
            config: RefCell::new(config),
            sessions_changed_subject,
            party_is_over_subject: Default::default(),
            control_surface_main_task_sender,
            #[cfg(feature = "playtime")]
            clip_matrix_event_sender,
            osc_feedback_task_sender,
            additional_feedback_event_sender,
            feedback_audio_hook_task_sender,
            instance_orchestration_event_sender,
            audio_hook_task_sender,
            instances: Default::default(),
            instances_changed_subject: Default::default(),
            message_panel: Default::default(),
            osc_feedback_processor: Rc::new(RefCell::new(osc_feedback_processor)),
            #[cfg(feature = "playtime")]
            clip_engine_hub: playtime_clip_engine::proto::ClipEngineHub::new(),
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
                    App::version()
                ));
            }
        }
    }

    pub fn get_temp_dir() -> Option<&'static TempDir> {
        static TEMP_DIR: Lazy<Option<TempDir>> =
            Lazy::new(|| tempfile::Builder::new().prefix("realearn-").tempdir().ok());
        TEMP_DIR.as_ref()
    }

    #[cfg(feature = "playtime")]
    fn init_clip_engine() {
        use playtime_clip_engine::ClipEngine;
        let license_manager = crate::infrastructure::data::LicenseManager::new(
            App::helgoboss_resource_dir_path().join("licensing.json"),
        );
        #[derive(Debug)]
        struct RealearnMetricsRecorder;
        impl playtime_clip_engine::MetricsRecorder for RealearnMetricsRecorder {
            fn record_duration(&self, id: &'static str, delta: std::time::Duration) {
                base::metrics_util::record_duration_internal(id, delta);
            }
        }
        let metrics_recorder: Option<playtime_clip_engine::StaticMetricsRecorder> =
            if base::metrics_util::metrics_are_enabled() {
                Some(&RealearnMetricsRecorder)
            } else {
                None
            };
        #[derive(Debug)]
        struct RealearnClipEngineIntegration;
        impl playtime_clip_engine::ClipEngineIntegration for RealearnClipEngineIntegration {
            fn export_to_clipboard(
                &self,
                item: &dyn playtime_clip_engine::PlaytimeItem,
            ) -> anyhow::Result<()> {
                let text = crate::infrastructure::ui::lua_serializer::to_string(item)?;
                crate::infrastructure::ui::copy_text_to_clipboard(text);
                Ok(())
            }
        }
        let args = playtime_clip_engine::ClipEngineInitArgs {
            available_licenses: license_manager.licenses(),
            tap_sound_file: Self::realearn_high_click_sound_path(),
            metrics_recorder,
            integration: Box::new(RealearnClipEngineIntegration),
        };
        ClipEngine::make_available_globally(|| ClipEngine::new(args));
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

    fn create_services(&self) -> RealearnServices {
        RealearnServices {
            #[cfg(feature = "playtime")]
            playtime_service: server::services::playtime_service::create_playtime_service(
                &self.clip_engine_hub,
            ),
        }
    }

    // Executed whenever the first ReaLearn instance is loaded.
    pub fn wake_up(&self) {
        let prev_state = self.state.replace(AppState::WakingUp);
        let AppState::Sleeping(mut sleeping_state) = prev_state else {
            panic!("App was not sleeping");
        };
        // Start thread for async deallocation
        let async_deallocation_thread = start_async_deallocation_thread(
            RealearnDeallocator::with_metrics("async_deallocation"),
            sleeping_state.async_deallocation_receiver,
        );
        // Start async runtime
        let async_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("ReaLearn async runtime")
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
            App::logger(),
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
            App::logger(),
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

    pub fn register_processor_couple(
        &self,
        instance_id: InstanceId,
        real_time_processor: SharedRealTimeProcessor,
        main_processor: MainProcessor<WeakSession>,
    ) {
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::AddRealTimeProcessor(
                instance_id,
                real_time_processor,
            ));
        self.control_surface_main_task_sender.0.send_complaining(
            RealearnControlSurfaceMainTask::AddMainProcessor(main_processor),
        );
    }

    pub fn unregister_processor_couple(&self, instance_id: InstanceId) {
        self.unregister_main_processor(&instance_id);
        self.unregister_real_time_processor(instance_id);
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
    fn unregister_real_time_processor(&self, instance_id: InstanceId) {
        self.audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::RemoveRealTimeProcessor(instance_id));
    }

    /// We remove the main processor synchronously because it allows us to keep its fail-fast
    /// behavior. E.g. we can still panic if DomainEventHandler (weak session) or channel
    /// receivers are gone because we know it's not supposed to happen. Also, unlike with
    /// real-time processor, whatever cleanup work is necessary, we can do right here because we
    /// are in main thread already.
    fn unregister_main_processor(&self, instance_id: &InstanceId) {
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            // Remove main processor.
            control_surface
                .middleware_mut()
                .remove_main_processor(instance_id);
        });
    }

    pub fn feedback_audio_hook_task_sender(
        &self,
    ) -> &SenderToRealTimeThread<FeedbackAudioHookTask> {
        &self.feedback_audio_hook_task_sender
    }

    #[cfg(feature = "playtime")]
    pub fn clip_matrix_event_sender(
        &self,
    ) -> &SenderToNormalThread<crate::domain::QualifiedClipMatrixEvent> {
        &self.clip_matrix_event_sender
    }

    #[cfg(feature = "playtime")]
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
    ) -> SenderToNormalThread<InstanceOrchestrationEvent> {
        self.instance_orchestration_event_sender.clone()
    }

    pub fn osc_feedback_task_sender(&self) -> &SenderToNormalThread<OscFeedbackTask> {
        &self.osc_feedback_task_sender
    }

    pub fn control_surface_main_task_sender(&self) -> &RealearnControlSurfaceMainTaskSender {
        &self.control_surface_main_task_sender
    }

    #[cfg(feature = "playtime")]
    pub fn clip_engine_hub(&self) -> &playtime_clip_engine::proto::ClipEngineHub {
        &self.clip_engine_hub
    }

    fn temporarily_reclaim_control_surface_ownership(
        &self,
        f: impl FnOnce(&mut RealearnControlSurface),
    ) {
        // Shortly reclaim ownership of the control surface by unregistering it.
        let prev_state = self.state.replace(AppState::Suspended);
        let awake_state = if let AppState::Awake(s) = prev_state {
            s
        } else {
            panic!("App was not awake when trying to suspend");
        };
        let mut session = Reaper::get().medium_session();
        let mut control_surface = unsafe {
            session
                .plugin_register_remove_csurf_inst(awake_state.control_surface_handle)
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
            audio_hook_handle: awake_state.audio_hook_handle,
            accelerator_handle: awake_state.accelerator_handle,
            async_deallocation_thread: awake_state.async_deallocation_thread,
            async_runtime: awake_state.async_runtime,
        };
        self.state.replace(AppState::Awake(awake_state));
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

    // TODO-medium Return a reference to a SharedControllerManager! Clients might just want to turn
    //  this into a weak one.
    pub fn controller_preset_manager(&self) -> SharedControllerPresetManager {
        self.controller_preset_manager.clone()
    }

    pub fn main_preset_manager(&self) -> SharedMainPresetManager {
        self.main_preset_manager.clone()
    }

    pub fn preset_manager(&self, compartment: Compartment) -> Box<dyn ExtendedPresetManager> {
        match compartment {
            Compartment::Controller => Box::new(self.controller_preset_manager()),
            Compartment::Main => Box::new(self.main_preset_manager()),
        }
    }

    pub fn preset_link_manager(&self) -> SharedPresetLinkManager {
        self.preset_link_manager.clone()
    }

    pub fn osc_device_manager(&self) -> SharedOscDeviceManager {
        self.osc_device_manager.clone()
    }

    pub fn do_with_osc_device(&self, dev_id: OscDeviceId, f: impl FnOnce(&mut OscDevice)) {
        let mut dev = App::get()
            .osc_device_manager()
            .borrow()
            .find_device_by_id(&dev_id)
            .unwrap()
            .clone();
        f(&mut dev);
        App::get()
            .osc_device_manager()
            .borrow_mut()
            .update_device(dev)
            .unwrap();
    }

    pub fn server(&self) -> &SharedRealearnServer {
        &self.server
    }

    pub fn config(&self) -> Ref<AppConfig> {
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
        self.change_config(AppConfig::enable_server);
        start_result
    }

    pub fn stop_server_persistently(&self) {
        self.change_config(AppConfig::disable_server);
        self.server.borrow_mut().stop();
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
            self.instances.borrow().len(),
            determine_module_base_address().map(|addr| format!("0x{addr:x}")),
        );
        Reaper::get().show_console_msg(msg);
        self.server.borrow().log_debug_info(session_id);
        self.controller_preset_manager.borrow().log_debug_info();
    }

    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.sessions_changed_subject.borrow().clone()
    }

    fn change_config(&self, op: impl FnOnce(&mut AppConfig)) {
        let mut config = self.config.borrow_mut();
        op(&mut config);
        config.save().unwrap();
        self.notify_changed();
    }

    fn helgoboss_resource_dir_path() -> PathBuf {
        Reaper::get().resource_path().join("Helgoboss")
    }

    #[cfg(feature = "playtime")]
    pub fn app_base_dir_path() -> PathBuf {
        App::helgoboss_resource_dir_path().join("App")
    }

    fn realearn_resource_dir_path() -> PathBuf {
        App::helgoboss_resource_dir_path().join("ReaLearn")
    }

    pub fn realearn_data_dir_path() -> PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/realearn")
    }

    #[cfg(feature = "playtime")]
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
            let dest_path = App::get_temp_dir()?.path().join("click-high.mp3");
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
            let dest_path = App::get_temp_dir()?.path().join("pot-preview.RPP");
            fs::write(&dest_path, bytes).ok()?;
            Some(dest_path)
        });
        PATH.as_ref().map(|p| p.as_path())
    }

    pub fn realearn_preset_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("presets")
    }

    pub fn realearn_auto_load_configs_dir_path() -> PathBuf {
        Self::realearn_data_dir_path().join("auto-load-configs")
    }

    pub fn realearn_osc_device_config_file_path() -> PathBuf {
        App::realearn_resource_dir_path().join("osc.json")
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

    #[cfg(feature = "playtime")]
    pub fn get_app_library() -> anyhow::Result<&'static crate::infrastructure::ui::AppLibrary> {
        use anyhow::Context;
        let app_library = APP_LIBRARY
            .get()
            .context("App not loaded yet. Please try again later.")?
            .as_ref();
        app_library.map_err(|e| anyhow::anyhow!(format!("{e:?}")))
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.find_session_by_id(session_id).is_some()
    }

    pub fn find_session_by_id(&self, session_id: &str) -> Option<SharedSession> {
        self.find_session(|session| {
            let Ok(session) = session.try_borrow() else {
                return false;
            };
            session.id() == session_id
        })
    }

    #[allow(dead_code)]
    pub fn find_main_panel_by_session_id(&self, session_id: &str) -> Option<SharedView<MainPanel>> {
        self.instances.borrow().iter().find_map(|i| {
            if i.session.upgrade()?.borrow().id() == session_id {
                i.ui.upgrade()
            } else {
                None
            }
        })
    }

    #[allow(dead_code)]
    pub fn find_main_panel_by_instance_id(
        &self,
        instance_id: InstanceId,
    ) -> Option<SharedView<MainPanel>> {
        self.instances
            .borrow()
            .iter()
            .find(|i| i.instance_id == instance_id)
            .and_then(|i| i.ui.upgrade())
    }

    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix<R>(
        &self,
        clip_matrix_id: &str,
        f: impl FnOnce(&playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let session = self
            .find_session_by_id(clip_matrix_id)
            .context("session not found")?;
        let session = session.borrow();
        let instance_state = session.instance_state();
        BackboneState::get().with_clip_matrix(instance_state, f)
    }

    #[cfg(feature = "playtime")]
    pub fn with_clip_matrix_mut<R>(
        &self,
        clip_matrix_id: &str,
        f: impl FnOnce(&mut playtime_clip_engine::base::Matrix) -> R,
    ) -> anyhow::Result<R> {
        let session = self
            .find_session_by_id(clip_matrix_id)
            .context("session not found")?;
        let session = session.borrow();
        let instance_state = session.instance_state();
        BackboneState::get().with_clip_matrix_mut(instance_state, f)
    }

    #[cfg(feature = "playtime")]
    pub fn create_clip_matrix(&self, clip_matrix_id: &str) -> anyhow::Result<()> {
        let session = self
            .find_session_by_id(clip_matrix_id)
            .context("session not found")?;
        let session = session.borrow();
        let instance_state = session.instance_state();
        BackboneState::get()
            .get_or_insert_owned_clip_matrix_from_instance_state(&mut instance_state.borrow_mut());
        Ok(())
    }

    pub fn find_session_by_id_ignoring_borrowed_ones(
        &self,
        session_id: &str,
    ) -> Option<SharedSession> {
        self.find_session(|session| {
            if let Ok(session) = session.try_borrow() {
                session.id() == session_id
            } else {
                false
            }
        })
    }

    pub fn find_session_id_by_instance_id(&self, instance_id: InstanceId) -> Option<String> {
        let session = self.find_session_by_instance_id_ignoring_borrowed_ones(instance_id)?;
        let session = session.borrow();
        Some(session.id().to_string())
    }

    pub fn find_instance_id_by_session_id(&self, session_id: &str) -> Option<InstanceId> {
        let session = self.find_session_by_id(session_id)?;
        let session = session.borrow();
        Some(*session.instance_id())
    }

    pub fn find_session_by_instance_id_ignoring_borrowed_ones(
        &self,
        instance_id: InstanceId,
    ) -> Option<SharedSession> {
        self.find_session(|session| {
            if let Ok(session) = session.try_borrow() {
                *session.instance_id() == instance_id
            } else {
                false
            }
        })
    }

    fn find_original_mapping(
        &self,
        initiator_instance_id: InstanceId,
        id: QualifiedMappingId,
    ) -> Result<SharedMapping, &'static str> {
        let session = self
            .find_session_by_instance_id_ignoring_borrowed_ones(initiator_instance_id)
            .ok_or("initiator session not found")?;
        let session = session.borrow();
        let mapping = session
            .find_mapping_by_id(id.compartment, id.id)
            .ok_or("origin mapping not found")?;
        Ok(mapping.clone())
    }

    pub fn find_session(
        &self,
        predicate: impl FnMut(&SharedSession) -> bool,
    ) -> Option<SharedSession> {
        self.instances
            .borrow()
            .iter()
            .filter_map(|s| s.session.upgrade())
            .find(predicate)
    }

    pub fn with_instances<R>(&self, f: impl FnOnce(&[PluginInstanceInfo]) -> R) -> R {
        f(&self.instances.borrow())
    }

    pub fn find_session_by_containing_fx(&self, fx: &Fx) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().containing_fx() == fx
        })
    }

    pub fn register_plugin_instance(&self, instance: PluginInstanceInfo) {
        let mut instances = self.instances.borrow_mut();
        debug!(Reaper::get().logger(), "Registering new session...");
        instances.push(instance);
        debug!(
            Reaper::get().logger(),
            "Session registered. Session count: {}",
            instances.len()
        );
        self.notify_instances_changed();
    }

    pub fn unregister_session(&self, session: *const Session) {
        let mut instances = self.instances.borrow_mut();
        debug!(Reaper::get().logger(), "Unregistering session...");
        instances.retain(|i| {
            match i.session.upgrade() {
                // Already gone, for whatever reason. Time to throw out!
                None => false,
                // Not gone yet.
                Some(shared_session) => shared_session.as_ptr() != session as _,
            }
        });
        debug!(
            Reaper::get().logger(),
            "Session unregistered. Remaining count of managed sessions: {}",
            instances.len()
        );
        self.notify_instances_changed();
    }

    fn notify_instances_changed(&self) {
        self.instances_changed_subject.borrow_mut().next(());
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

    fn register_actions(control_surface_sender: &RealearnControlSurfaceMainTaskSender) {
        let control_surface_sender = control_surface_sender.0.clone();
        Reaper::get().register_action(
            "realearnLearnSourceForLastTouchedTarget",
            "ReaLearn: Learn source for last touched target (reassigning target)",
            move || {
                let included_target_types = ReaperTargetType::into_enum_iter().collect();
                let filter = LastTouchedTargetFilter {
                    included_target_types: &included_target_types,
                    touch_cause: TargetTouchCause::Any,
                };
                let target = BackboneState::get().find_last_touched_target(filter);
                let target = match target.as_ref() {
                    None => return,
                    Some(t) => t,
                };
                App::get().start_learning_source_for_target(Compartment::Main, target);
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE",
            "ReaLearn: Learn single mapping (reassigning source)",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .learn_mapping_reassigning_source(Compartment::Main, false)
                        .await;
                    Ok(())
                });
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE_OPEN",
            "ReaLearn: Learn single mapping (reassigning source) and open it",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .learn_mapping_reassigning_source(Compartment::Main, true)
                        .await;
                    Ok(())
                });
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_FIND_FIRST_MAPPING_BY_SOURCE",
            "ReaLearn: Find first mapping by source",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .find_first_mapping_by_source(Compartment::Main)
                        .await;
                    Ok(())
                });
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_OPEN_FIRST_POT_BROWSER",
            "ReaLearn: Open first Pot Browser",
            move || {
                let Some(session) = App::get().find_first_relevant_session_monitoring_first()
                else {
                    return;
                };
                session.borrow().ui().show_pot_browser();
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_FIND_FIRST_MAPPING_BY_TARGET",
            "ReaLearn: Find first mapping by target",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .find_first_mapping_by_target(Compartment::Main)
                        .await;
                    Ok(())
                });
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_SEND_ALL_FEEDBACK",
            "ReaLearn: Send feedback for all instances",
            move || {
                control_surface_sender
                    .send_complaining(RealearnControlSurfaceMainTask::SendAllFeedback);
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_RESOLVE_SYMBOLS",
            "[developer] ReaLearn: Resolve symbols from clipboard",
            || {
                if let Err(e) = resolve_symbols_from_clipboard() {
                    Reaper::get().show_console_msg(format!("{e}\n"));
                }
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_INTEGRATION_TEST",
            "[developer] ReaLearn: Run integration test",
            run_test,
            ActionKind::NotToggleable,
        );
    }

    async fn find_first_mapping_by_source(
        &self,
        compartment: Compartment,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some control elements!", || {
            App::stop_learning_sources();
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

    async fn find_first_mapping_by_target(
        &self,
        compartment: Compartment,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some targets!", || {
            App::get()
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

    async fn learn_mapping_reassigning_source(
        &self,
        compartment: Compartment,
        open_mapping: bool,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        if self.find_first_relevant_session_project_first().is_none() {
            self.close_message_panel_with_alert(
                "At first you need to add a ReaLearn instance to the monitoring FX chain or this project! Don't forget to set the MIDI control input.",
            );
            return Err("no ReaLearn instance");
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
                "No ReaLearn instance found which has this MIDI control input! First please add one to the monitoring FX chain or this project and set the MIDI control input accordingly!",
            );
            return Err("no ReaLearn instance with that MIDI input");
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
            App::stop_learning_sources();
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
        App::get()
            .audio_hook_task_sender
            .send_complaining(NormalAudioHookTask::StopCapturingMidi);
        App::get()
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

    async fn prompt_for_next_reaper_target(&self, msg: &str) -> Result<ReaperTarget, &'static str> {
        self.show_message_panel("ReaLearn", msg, || {
            App::get()
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

    fn start_learning_source_for_target(&self, compartment: Compartment, target: &ReaperTarget) {
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
                    "No suitable ReaLearn instance found! First please add one to the monitoring FX chain or this project!",
                );
            }
            Some(s) => {
                let mapping =
                    s.borrow_mut()
                        .toggle_learn_source_for_target(&s, compartment, target);
                s.borrow().show_mapping(compartment, mapping.borrow().id());
            }
        }
    }

    fn find_first_relevant_session_with_target(
        &self,
        compartment: Compartment,
        target: &ReaperTarget,
    ) -> Option<(SharedSession, SharedMapping)> {
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
        compartment: Compartment,
        target: &ReaperTarget,
    ) -> Option<(SharedSession, SharedMapping)> {
        self.instances.borrow().iter().find_map(|session| {
            let session = session.session.upgrade()?;
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

    fn find_first_session_on_track(&self, track: &Track) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().track() == Some(track)
        })
    }

    fn find_first_relevant_session_monitoring_first(&self) -> Option<SharedSession> {
        self.find_first_session_in_project(None)
            .or_else(|| self.find_first_session_in_project(Some(Reaper::get().current_project())))
    }

    fn find_first_relevant_session_project_first(&self) -> Option<SharedSession> {
        self.find_first_session_in_project(Some(Reaper::get().current_project()))
            .or_else(|| self.find_first_session_in_project(None))
    }

    /// Project None means monitoring FX chain.
    fn find_first_session_in_project(&self, project: Option<Project>) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().project() == project
        })
    }

    fn find_first_relevant_session_with_input_from(
        &self,
        input_descriptor: &InputDescriptor,
    ) -> Option<SharedSession> {
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
    ) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.processor_context().project() == project
                && session.receives_input_from(input_descriptor)
        })
    }

    fn find_first_relevant_session_with_source_matching(
        &self,
        compartment: Compartment,
        capture_result: &MessageCaptureResult,
    ) -> Option<(SharedSession, SharedMapping)> {
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
        compartment: Compartment,
        capture_result: &MessageCaptureResult,
    ) -> Option<(SharedSession, SharedMapping)> {
        self.instances.borrow().iter().find_map(|session| {
            let session = session.session.upgrade()?;
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
        f: impl Fn(&mut Session, WeakSession),
    ) -> Result<(), &'static str> {
        if common_args.scope.has_tags() {
            // Modify all sessions whose tags match.
            for instance in self.instances.borrow().iter() {
                if let Some(session) = instance.session.upgrade() {
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
                    f(&mut session, instance.session.clone())
                }
            }
        } else {
            // Modify the initiator session only.
            let shared_session = self
                .find_session_by_instance_id_ignoring_borrowed_ones(
                    common_args.initiator_instance_id,
                )
                .ok_or("initiator session not found")?;
            let mut session = shared_session.borrow_mut();
            f(&mut session, Rc::downgrade(&shared_session));
        }
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.message_panel.close();
        self.party_is_over_subject.next(());
        let _ = unregister_api();
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    main: MainConfig,
}

impl AppConfig {
    pub fn load() -> Result<AppConfig, String> {
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

    fn config_file_path() -> PathBuf {
        App::realearn_resource_dir_path().join("realearn.ini")
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
            server_enabled: Default::default(),
            server_http_port: default_server_http_port(),
            server_https_port: default_server_https_port(),
            server_grpc_port: default_server_grpc_port(),
            companion_web_app_url: default_companion_web_app_url(),
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
    format!(
        "v{}/{}{} rev {}{} ({})",
        PKG_VERSION,
        CFG_TARGET_ARCH,
        debug_mark,
        GIT_COMMIT_HASH
            .map(|h| h[0..6].to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        dirty_mark,
        date_info
    )
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

impl HookPostCommand for App {
    fn call(command_id: CommandId, _flag: i32) {
        App::get()
            .additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                section_context: SectionContext::MainSection,
                command_id,
            }));
    }
}

impl HookPostCommand2 for App {
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
        App::get()
            .additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                section_context: SectionContext::MainSection,
                command_id,
            }));
    }
}

impl InstanceContainer for App {
    fn find_session_by_id(&self, session_id: &str) -> Option<SharedSession> {
        App::get().find_session_by_id_ignoring_borrowed_ones(session_id)
    }

    fn find_session_by_instance_id(&self, instance_id: InstanceId) -> Option<SharedSession> {
        App::get().find_session_by_instance_id_ignoring_borrowed_ones(instance_id)
    }

    fn enable_instances(&self, args: EnableInstancesArgs) -> Option<HashSet<Tag>> {
        let mut activated_inverse_tags = HashSet::new();
        for session in self.instances.borrow().iter() {
            if let Some(session) = session.session.upgrade() {
                let session = session.borrow();
                // Don't touch ourselves.
                if *session.instance_id() == args.common.initiator_instance_id {
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
pub struct RealearnSnitch;

impl RealearnWindowSnitch for RealearnSnitch {
    fn find_closest_realearn_view(&self, window: Window) -> Option<SharedView<dyn View>> {
        let view_manager = ViewManager::get().borrow();
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

#[cfg(feature = "playtime")]
fn load_app_library() -> anyhow::Result<crate::infrastructure::ui::AppLibrary> {
    tracing::info!("Loading app library...");
    let app_base_dir = App::app_base_dir_path();
    let lib = crate::infrastructure::ui::AppLibrary::load(app_base_dir);
    match lib.as_ref() {
        Ok(_) => {
            tracing::info!("App library loaded successfully");
        }
        Err(e) => {
            tracing::warn!("App library loading failed: {e}");
        }
    }
    lib
}

#[cfg(feature = "playtime")]
fn decompress_app() -> anyhow::Result<()> {
    // Check if decompression necessary
    use anyhow::Context;
    let archive_file = &App::app_archive_file_path();
    let destination_dir = &App::app_base_dir_path();
    let archive_metadata = archive_file.metadata()?;
    let archive_size = archive_metadata.len();
    let archive_modified = archive_metadata
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)?;
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
