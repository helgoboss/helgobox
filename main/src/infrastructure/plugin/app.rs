use crate::application::{
    GroupId, InputDescriptor, Session, SharedMapping, SharedSession, VirtualControlElementType,
    WeakSession,
};
use crate::core::default_util::is_default;
use crate::core::{notification, Global};
use crate::domain::{
    ActionInvokedEvent, AdditionalFeedbackEvent, BackboneState, FeedbackAudioHookTask, Garbage,
    InstanceId, InstanceOrchestrationEvent, MainProcessor, MappingCompartment, MidiSource,
    NormalAudioHookTask, OscDeviceId, OscFeedbackProcessor, OscFeedbackTask, RealSource,
    RealTimeSender, RealearnAudioHook, RealearnControlSurfaceMainTask,
    RealearnControlSurfaceMiddleware, RealearnControlSurfaceServerTask, RealearnTargetContext,
    ReaperTarget, SharedRealTimeProcessor,
};
use crate::infrastructure::data::{
    FileBasedControllerPresetManager, FileBasedMainPresetManager, FileBasedPresetLinkManager,
    OscDevice, OscDeviceManager, SharedControllerPresetManager, SharedMainPresetManager,
    SharedOscDeviceManager, SharedPresetLinkManager,
};
use crate::infrastructure::plugin::debug_util;
use crate::infrastructure::server;
use crate::infrastructure::server::{RealearnServer, SharedRealearnServer, COMPANION_WEB_APP_URL};
use crate::infrastructure::ui::MessagePanel;
use helgoboss_learn::OscSource;

use basedrop::Owned;
use reaper_high::{ActionKind, CrashInfo, Fx, MiddlewareControlSurface, Project, Reaper, Track};
use reaper_low::{PluginContext, Swell};
use reaper_medium::{
    ActionValueChange, CommandId, HookPostCommand, HookPostCommand2, MidiInputDeviceId, ReaProject,
    RegistrationHandle, SectionContext, WindowContext,
};
use reaper_rx::{ActionRxHookPostCommand, ActionRxHookPostCommand2};
use rx_util::UnitEvent;
use rxrust::prelude::*;
use semver::Version;
use serde::{Deserialize, Serialize};
use slog::{debug, Drain, Logger};
use std::cell::{Ref, RefCell};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use swell_ui::{SharedView, View};
use url::Url;
use wrap_debug::WrapDebug;

const CONTROL_SURFACE_MAIN_TASK_QUEUE_SIZE: usize = 500;
const CONTROL_SURFACE_SERVER_TASK_QUEUE_SIZE: usize = 500;
// Probably can get quite much on action invocation.
// https://github.com/helgoboss/realearn/issues/234
const ADDITIONAL_FEEDBACK_EVENT_QUEUE_SIZE: usize = 20_000;
// If we have very many instances, this might not be enough. But the task size is so
// small, so why not make it a great number? It's global, not per instance. For one
// instance we had 2000 before and it worked great. With 100_000 we can easily cover 50 instances
// and yet it's only around 1 MB memory usage (globally). We are on the safe side!
const FEEDBACK_AUDIO_HOOK_TASK_QUEUE_SIZE: usize = 100_000;
const GARBAGE_QUEUE_SIZE: usize = 100_000;
const INSTANCE_ORCHESTRATION_EVENT_QUEUE_SIZE: usize = 5000;
const NORMAL_AUDIO_HOOK_TASK_QUEUE_SIZE: usize = 2000;
const OSC_OUTGOING_QUEUE_SIZE: usize = 1000;

make_available_globally_in_main_thread!(App);

pub type RealearnControlSurface =
    MiddlewareControlSurface<RealearnControlSurfaceMiddleware<WeakSession>>;

pub type RealearnControlSurfaceMainTaskSender =
    crossbeam_channel::Sender<RealearnControlSurfaceMainTask<WeakSession>>;

pub type RealearnControlSurfaceServerTaskSender =
    crossbeam_channel::Sender<RealearnControlSurfaceServerTask>;

pub struct App {
    state: RefCell<AppState>,
    controller_preset_manager: SharedControllerPresetManager,
    main_preset_manager: SharedMainPresetManager,
    preset_link_manager: SharedPresetLinkManager,
    osc_device_manager: SharedOscDeviceManager,
    server: SharedRealearnServer,
    config: RefCell<AppConfig>,
    changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    list_of_recently_focused_fx: Rc<RefCell<ListOfRecentlyFocusedFx>>,
    party_is_over_subject: LocalSubject<'static, (), ()>,
    control_surface_main_task_sender: RealearnControlSurfaceMainTaskSender,
    osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
    additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    feedback_audio_hook_task_sender: RealTimeSender<FeedbackAudioHookTask>,
    instance_orchestration_event_sender: crossbeam_channel::Sender<InstanceOrchestrationEvent>,
    audio_hook_task_sender: crossbeam_channel::Sender<NormalAudioHookTask>,
    sessions: RefCell<Vec<WeakSession>>,
    sessions_changed_subject: RefCell<LocalSubject<'static, (), ()>>,
    message_panel: SharedView<MessagePanel>,
    osc_feedback_processor: Rc<RefCell<OscFeedbackProcessor>>,
}

#[derive(Debug)]
enum AppState {
    /// Start state.
    ///
    /// Entered only once at startup.
    Uninitialized(UninitializedState),
    /// During initialization as soon as we have access to REAPER.
    ///
    /// Entered only once at startup.
    Initializing,
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
struct UninitializedState {
    control_surface_main_task_receiver:
        crossbeam_channel::Receiver<RealearnControlSurfaceMainTask<WeakSession>>,
    control_surface_server_task_receiver:
        crossbeam_channel::Receiver<RealearnControlSurfaceServerTask>,
    additional_feedback_event_receiver: crossbeam_channel::Receiver<AdditionalFeedbackEvent>,
    instance_orchestration_event_receiver: crossbeam_channel::Receiver<InstanceOrchestrationEvent>,
    normal_audio_hook_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_audio_hook_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
}

#[derive(Debug)]
struct SleepingState {
    control_surface: Box<RealearnControlSurface>,
    audio_hook: Box<RealearnAudioHook>,
}

#[derive(Debug)]
struct AwakeState {
    control_surface_handle: RegistrationHandle<RealearnControlSurface>,
    audio_hook_handle: RegistrationHandle<RealearnAudioHook>,
    collector_handle: WrapDebug<basedrop::Handle>,
}

impl Default for App {
    fn default() -> Self {
        // TODO-low Not so super cool to load from a file in the default function. However,
        //  that made it easier for our make_available_globally_in_main_thread!().
        let config = AppConfig::load().unwrap_or_else(|e| {
            debug!(App::logger(), "{}", e);
            Default::default()
        });
        App::new(config)
    }
}

#[derive(Default)]
struct ListOfRecentlyFocusedFx {
    previous: Option<Fx>,
    current: Option<Fx>,
}

impl ListOfRecentlyFocusedFx {
    fn feed(&mut self, currently_focused_fx: Option<Fx>) {
        self.previous = self.current.take();
        self.current = currently_focused_fx;
    }
}

impl App {
    pub fn detailed_version_label() -> &'static str {
        use once_cell::sync::Lazy;
        static VALUE: Lazy<String> = Lazy::new(build_detailed_version);
        &VALUE
    }

    pub fn version() -> &'static Version {
        use once_cell::sync::Lazy;
        static VALUE: Lazy<Version> = Lazy::new(|| {
            Version::parse(crate::infrastructure::plugin::built_info::PKG_VERSION).unwrap()
        });
        &VALUE
    }

    pub fn given_version_is_newer_than_app_version(version: Option<&Version>) -> bool {
        if let Some(v) = version {
            Self::version() < v
        } else {
            false
        }
    }

    fn new(config: AppConfig) -> App {
        let (main_sender, main_receiver) =
            crossbeam_channel::bounded(CONTROL_SURFACE_MAIN_TASK_QUEUE_SIZE);
        let (server_sender, server_receiver) =
            crossbeam_channel::bounded(CONTROL_SURFACE_SERVER_TASK_QUEUE_SIZE);
        let (osc_feedback_task_sender, osc_feedback_task_receiver) =
            crossbeam_channel::bounded(OSC_OUTGOING_QUEUE_SIZE);
        let (additional_feedback_event_sender, additional_feedback_event_receiver) =
            crossbeam_channel::bounded(ADDITIONAL_FEEDBACK_EVENT_QUEUE_SIZE);
        let (instance_orchestration_event_sender, instance_orchestration_event_receiver) =
            crossbeam_channel::bounded(INSTANCE_ORCHESTRATION_EVENT_QUEUE_SIZE);
        let (feedback_audio_hook_task_sender, feedback_audio_hook_task_receiver) =
            crossbeam_channel::bounded(FEEDBACK_AUDIO_HOOK_TASK_QUEUE_SIZE);
        let (audio_sender, audio_receiver) =
            crossbeam_channel::bounded(NORMAL_AUDIO_HOOK_TASK_QUEUE_SIZE);
        let uninitialized_state = UninitializedState {
            control_surface_main_task_receiver: main_receiver,
            control_surface_server_task_receiver: server_receiver,
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            normal_audio_hook_task_receiver: audio_receiver,
            feedback_audio_hook_task_receiver,
        };
        App {
            state: RefCell::new(AppState::Uninitialized(uninitialized_state)),
            controller_preset_manager: Rc::new(RefCell::new(
                FileBasedControllerPresetManager::new(
                    App::realearn_preset_dir_path().join("controller"),
                ),
            )),
            main_preset_manager: Rc::new(RefCell::new(FileBasedMainPresetManager::new(
                App::realearn_preset_dir_path().join("main"),
            ))),
            preset_link_manager: Rc::new(RefCell::new(FileBasedPresetLinkManager::new(
                App::realearn_auto_load_configs_dir_path(),
            ))),
            osc_device_manager: Rc::new(RefCell::new(OscDeviceManager::new(
                App::realearn_osc_device_config_file_path(),
            ))),
            server: Rc::new(RefCell::new(RealearnServer::new(
                config.main.server_http_port,
                config.main.server_https_port,
                App::server_resource_dir_path().join("certificates"),
                server_sender,
            ))),
            config: RefCell::new(config),
            changed_subject: Default::default(),
            list_of_recently_focused_fx: Default::default(),
            party_is_over_subject: Default::default(),
            control_surface_main_task_sender: main_sender,
            osc_feedback_task_sender,
            additional_feedback_event_sender,
            feedback_audio_hook_task_sender: RealTimeSender::new(feedback_audio_hook_task_sender),
            instance_orchestration_event_sender,
            audio_hook_task_sender: audio_sender,
            sessions: Default::default(),
            sessions_changed_subject: Default::default(),
            message_panel: Default::default(),
            osc_feedback_processor: Rc::new(RefCell::new(OscFeedbackProcessor::new(
                osc_feedback_task_receiver,
            ))),
        }
    }

    /// Executed globally just once when module loaded.
    pub fn init_static(logger: Logger, context: PluginContext) {
        Swell::make_available_globally(Swell::load(context));
        Reaper::setup_with_defaults(
            context,
            logger,
            CrashInfo {
                plugin_name: "ReaLearn".to_string(),
                plugin_version: App::detailed_version_label().to_string(),
                support_email_address: "info@helgoboss.org".to_string(),
            },
        );
        App::get().init();
    }

    /// Executed globally just once as soon as we have access to global REAPER instance.
    pub fn init(&self) {
        let prev_state = self.state.replace(AppState::Initializing);
        let uninit_state = if let AppState::Uninitialized(s) = prev_state {
            s
        } else {
            panic!("App was not uninitialized anymore");
        };
        BackboneState::make_available_globally(BackboneState::new(RealearnTargetContext::new(
            self.additional_feedback_event_sender.clone(),
        )));
        App::get().register_actions();
        server::keep_informing_clients_about_sessions();
        debug_util::register_resolve_symbols_action();
        crate::infrastructure::test::register_test_action();
        let list_of_recently_focused_fx = self.list_of_recently_focused_fx.clone();
        self.osc_device_manager
            .borrow()
            .changed()
            .subscribe(|_| App::get().reconnect_osc_devices());
        Global::control_surface_rx()
            .fx_focused()
            .take_until(self.party_is_over())
            .subscribe(move |fx| {
                list_of_recently_focused_fx.borrow_mut().feed(fx);
            });
        let control_surface = MiddlewareControlSurface::new(RealearnControlSurfaceMiddleware::new(
            &App::logger(),
            uninit_state.control_surface_main_task_receiver,
            uninit_state.control_surface_server_task_receiver,
            uninit_state.additional_feedback_event_receiver,
            uninit_state.instance_orchestration_event_receiver,
            Self::garbage_channel().1.clone(),
            std::env::var("REALEARN_METER").is_ok(),
        ));
        let audio_hook = RealearnAudioHook::new(
            uninit_state.normal_audio_hook_task_receiver,
            uninit_state.feedback_audio_hook_task_receiver,
            Self::garbage_sender().clone(),
        );
        let sleeping_state = SleepingState {
            control_surface: Box::new(control_surface),
            audio_hook: Box::new(audio_hook),
        };
        self.state.replace(AppState::Sleeping(sleeping_state));
    }

    fn reconnect_osc_devices(&self) {
        // Control devices
        self.temporarily_reclaim_control_surface_ownership(|control_surface| {
            let middleware = control_surface.middleware_mut();
            // Disconnect
            middleware.clear_osc_input_devices();
            // Reconnect
            let osc_input_devices = self
                .osc_device_manager
                .borrow_mut()
                .connect_all_enabled_inputs();
            middleware.set_osc_input_devices(osc_input_devices);
        });
        // Feedback devices
        {
            // Disconnect
            let mut processor = self.osc_feedback_processor.borrow_mut();
            processor.stop();
            // Reconnect
            let osc_output_devices = self
                .osc_device_manager
                .borrow_mut()
                .connect_all_enabled_outputs();
            processor.start(osc_output_devices);
        }
    }

    // Executed whenever the first ReaLearn instance is loaded.
    pub fn wake_up(&self) {
        let prev_state = self.state.replace(AppState::WakingUp);
        let mut sleeping_state = if let AppState::Sleeping(s) = prev_state {
            s
        } else {
            panic!("App was not sleeping");
        };
        if self.config.borrow().server_is_enabled() {
            self.server()
                .borrow_mut()
                .start()
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
        let osc_input_devices = self
            .osc_device_manager
            .borrow_mut()
            .connect_all_enabled_inputs();
        let osc_output_devices = self
            .osc_device_manager
            .borrow_mut()
            .connect_all_enabled_outputs();
        // OSC processor
        self.osc_feedback_processor
            .borrow_mut()
            .start(osc_output_devices);
        // Control surface
        let middleware = sleeping_state.control_surface.middleware_mut();
        middleware.set_osc_input_devices(osc_input_devices);
        let collector_handle = middleware.collector_handle();
        sleeping_state.control_surface.middleware().wake_up();
        let control_surface_handle = session
            .plugin_register_add_csurf_inst(sleeping_state.control_surface)
            .expect("couldn't register ReaLearn control surface");
        // Awake state
        let awake_state = AwakeState {
            control_surface_handle,
            audio_hook_handle,
            collector_handle: WrapDebug(collector_handle),
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
        let (mut control_surface, audio_hook) = unsafe {
            let control_surface = session
                .plugin_register_remove_csurf_inst(awake_state.control_surface_handle)
                .expect("control surface was not registered");
            let audio_hook = session
                .audio_reg_hardware_hook_remove(awake_state.audio_hook_handle)
                .expect("control surface was not registered");
            (control_surface, audio_hook)
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
        let sleeping_state = SleepingState {
            control_surface,
            audio_hook,
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
            .try_send(NormalAudioHookTask::AddRealTimeProcessor(
                instance_id,
                WrapDebug(Owned::new(&self.collector_handle(), real_time_processor)),
            ))
            .unwrap();
        self.control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::AddMainProcessor(
                main_processor,
            ))
            .unwrap();
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
            .try_send(NormalAudioHookTask::RemoveRealTimeProcessor(instance_id))
            .unwrap();
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

    pub fn feedback_audio_hook_task_sender(&self) -> &RealTimeSender<FeedbackAudioHookTask> {
        &self.feedback_audio_hook_task_sender
    }

    pub fn additional_feedback_event_sender(
        &self,
    ) -> crossbeam_channel::Sender<AdditionalFeedbackEvent> {
        self.additional_feedback_event_sender.clone()
    }

    pub fn instance_orchestration_event_sender(
        &self,
    ) -> crossbeam_channel::Sender<InstanceOrchestrationEvent> {
        self.instance_orchestration_event_sender.clone()
    }

    pub fn osc_feedback_task_sender(&self) -> &crossbeam_channel::Sender<OscFeedbackTask> {
        &self.osc_feedback_task_sender
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
            collector_handle: awake_state.collector_handle,
        };
        self.state.replace(AppState::Awake(awake_state));
    }

    /// The special thing about this is that this doesn't return the currently focused FX but the
    /// last focused one. That's important because when queried from ReaLearn UI, the current one
    /// is mostly ReaLearn itself - which is in most cases not what we want.
    pub fn previously_focused_fx(&self) -> Option<Fx> {
        self.list_of_recently_focused_fx.borrow().previous.clone()
    }

    // TODO-medium Return a reference to a SharedControllerManager! Clients might just want to turn
    //  this into a weak one.
    pub fn controller_preset_manager(&self) -> SharedControllerPresetManager {
        self.controller_preset_manager.clone()
    }

    pub fn main_preset_manager(&self) -> SharedMainPresetManager {
        self.main_preset_manager.clone()
    }

    pub fn preset_link_manager(&self) -> SharedPresetLinkManager {
        self.preset_link_manager.clone()
    }

    pub fn collector_handle(&self) -> basedrop::Handle {
        let state = self.state.borrow();
        if let AppState::Awake(s) = &*state {
            s.collector_handle.0.clone()
        } else {
            panic!("attempted to get collector handle while not being awake")
        }
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

    pub fn start_server_persistently(&self) -> Result<(), String> {
        self.server.borrow_mut().start()?;
        self.change_config(AppConfig::enable_server);
        Ok(())
    }

    pub fn disable_server_persistently(&self) {
        self.change_config(AppConfig::disable_server);
    }

    pub fn enable_server_persistently(&self) {
        self.change_config(AppConfig::enable_server);
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
            self.sessions.borrow().len(),
            determine_module_base_address().map(|addr| format!("0x{:x}", addr)),
        );
        Reaper::get().show_console_msg(msg);
        self.server.borrow().log_debug_info(session_id);
        self.controller_preset_manager.borrow().log_debug_info();
        self.control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::LogDebugInfo)
            .unwrap();
    }

    pub fn changed(&self) -> impl UnitEvent {
        self.changed_subject.borrow().clone()
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

    fn realearn_resource_dir_path() -> PathBuf {
        App::helgoboss_resource_dir_path().join("ReaLearn")
    }

    pub fn realearn_data_dir_path() -> PathBuf {
        Reaper::get()
            .resource_path()
            .join("Data/helgoboss/realearn")
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

    // We need this to be static because we need it at plugin construction time, so we don't have
    // REAPER API access yet.
    pub fn garbage_sender() -> &'static crossbeam_channel::Sender<Garbage> {
        &Self::garbage_channel().0
    }

    fn garbage_channel() -> &'static (
        crossbeam_channel::Sender<Garbage>,
        crossbeam_channel::Receiver<Garbage>,
    ) {
        static CHANNEL: once_cell::sync::Lazy<(
            crossbeam_channel::Sender<Garbage>,
            crossbeam_channel::Receiver<Garbage>,
        )> = once_cell::sync::Lazy::new(|| crossbeam_channel::bounded(GARBAGE_QUEUE_SIZE));
        &CHANNEL
    }

    pub fn sessions_changed(&self) -> impl UnitEvent {
        self.changed_subject.borrow().clone()
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.find_session_by_id(session_id).is_some()
    }

    pub fn find_session_by_id(&self, session_id: &str) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.id() == session_id
        })
    }

    pub fn register_session(&self, session: WeakSession) {
        let mut sessions = self.sessions.borrow_mut();
        debug!(Reaper::get().logger(), "Registering new session...");
        sessions.push(session);
        debug!(
            Reaper::get().logger(),
            "Session registered. Session count: {}",
            sessions.len()
        );
        self.notify_sessions_changed();
    }

    pub fn unregister_session(&self, session: *const Session) {
        let mut sessions = self.sessions.borrow_mut();
        debug!(Reaper::get().logger(), "Unregistering session...");
        sessions.retain(|s| {
            match s.upgrade() {
                // Already gone, for whatever reason. Time to throw out!
                None => false,
                // Not gone yet.
                Some(shared_session) => shared_session.as_ptr() != session as _,
            }
        });
        debug!(
            Reaper::get().logger(),
            "Session unregistered. Remaining count of managed sessions: {}",
            sessions.len()
        );
        self.notify_sessions_changed();
    }

    fn notify_sessions_changed(&self) {
        self.sessions_changed_subject.borrow_mut().next(());
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

    pub fn register_actions(&self) {
        Reaper::get().register_action(
            "realearnLearnSourceForLastTouchedTarget",
            "ReaLearn: Learn source for last touched target (reassigning target)",
            move || {
                let target = BackboneState::get().last_touched_target();
                let target = match target.as_ref() {
                    None => return,
                    Some(t) => t,
                };
                App::get()
                    .start_learning_source_for_target(MappingCompartment::MainMappings, &target);
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_LEARN_MAPPING_REASSIGNING_SOURCE",
            "ReaLearn: Learn single mapping (reassigning source)",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .learn_mapping_reassigning_source(MappingCompartment::MainMappings, false)
                        .await;
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
                        .learn_mapping_reassigning_source(MappingCompartment::MainMappings, true)
                        .await;
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
                        .find_first_mapping_by_source(MappingCompartment::MainMappings)
                        .await;
                });
            },
            ActionKind::NotToggleable,
        );
        Reaper::get().register_action(
            "REALEARN_FIND_FIRST_MAPPING_BY_TARGET",
            "ReaLearn: Find first mapping by target",
            move || {
                Global::future_support().spawn_in_main_thread_from_main_thread(async {
                    let _ = App::get()
                        .find_first_mapping_by_target(MappingCompartment::MainMappings)
                        .await;
                });
            },
            ActionKind::NotToggleable,
        );
        let control_surface_sender = self.control_surface_main_task_sender.clone();
        Reaper::get().register_action(
            "REALEARN_SEND_ALL_FEEDBACK",
            "ReaLearn: Send feedback for all instances",
            move || {
                control_surface_sender
                    .try_send(RealearnControlSurfaceMainTask::SendAllFeedback)
                    .unwrap();
            },
            ActionKind::NotToggleable,
        );
    }

    async fn find_first_mapping_by_source(
        &self,
        compartment: MappingCompartment,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some control elements!", || {
            App::stop_learning_sources();
        });
        let midi_receiver = self.request_next_midi_sources();
        let osc_receiver = self.request_next_osc_sources();
        loop {
            let next_source = tokio::select! {
                Ok((dev_id, source)) = midi_receiver.recv() => {
                    Some(QualifiedRealSource::Midi(dev_id, source))
                }
                Ok((dev_id, source)) = osc_receiver.recv() => {
                    Some(QualifiedRealSource::Osc(dev_id, source))
                }
                else => None
            };
            if let Some(s) = next_source {
                if let Some((session, mapping)) =
                    self.find_first_relevant_session_with_source(compartment, &s)
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
        compartment: MappingCompartment,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        self.show_message_panel("ReaLearn", "Touch some targets!", || {
            App::stop_learning_targets();
        });
        let receiver = self.request_next_reaper_targets();
        while let Ok(target) = receiver.recv().await {
            if let Some((session, mapping)) =
                self.find_first_relevant_session_with_target(compartment, &target)
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
        compartment: MappingCompartment,
        open_mapping: bool,
    ) -> Result<(), &'static str> {
        self.toggle_guard()?;
        if self.find_first_relevant_session().is_none() {
            self.close_message_panel_with_alert(
                "At first you need to add a ReaLearn instance to the monitoring FX chain or this project! Don't forget to set the MIDI control input.",
            );
            return Err("no ReaLearn instance");
        }
        let real_source = self
            .prompt_for_next_real_source("Touch a control element!")
            .await?;
        let session = if let Some(s) =
            self.find_first_relevant_session_with_input_from(&real_source)
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
            self.find_first_relevant_session_with_source(compartment, &real_source)
        {
            // There's already a mapping with that source. Change target of that mapping.
            mapping
                .borrow_mut()
                .target_model
                .apply_from_target(&reaper_target, session.borrow().context());
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
                let compound_source =
                    s.create_compound_source(real_source.into_unqualified_real_source(), true);
                m.source_model.apply_from_source(&compound_source);
                m.target_model
                    .apply_from_target(&reaper_target, s.context());
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

    async fn prompt_for_next_real_source(
        &self,
        msg: &str,
    ) -> Result<QualifiedRealSource, &'static str> {
        self.show_message_panel("ReaLearn", msg, || {
            App::stop_learning_sources();
        });
        let midi_receiver = self.request_next_midi_sources();
        let osc_receiver = self.request_next_osc_sources();
        tokio::select! {
            Ok((dev_id, source)) = midi_receiver.recv() => {
                Ok(QualifiedRealSource::Midi(dev_id, source))
            }
            Ok((dev_id, source)) = osc_receiver.recv() => {
                Ok(QualifiedRealSource::Osc(dev_id, source))
            }
            else => Err("stopped learning")
        }
    }

    fn stop_learning_sources() {
        App::get()
            .audio_hook_task_sender
            .try_send(NormalAudioHookTask::StopLearningSources)
            .unwrap();
        App::get()
            .control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::StopLearning)
            .unwrap();
    }

    fn request_next_midi_sources(
        &self,
    ) -> async_channel::Receiver<(MidiInputDeviceId, MidiSource)> {
        let (sender, receiver) = async_channel::bounded(500);
        self.audio_hook_task_sender
            .try_send(NormalAudioHookTask::StartLearningSources(sender))
            .unwrap();
        receiver
    }

    fn request_next_osc_sources(&self) -> async_channel::Receiver<(OscDeviceId, OscSource)> {
        let (sender, receiver) = async_channel::bounded(500);
        self.control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::StartLearningSources(sender))
            .unwrap();
        receiver
    }

    async fn prompt_for_next_reaper_target(&self, msg: &str) -> Result<ReaperTarget, &'static str> {
        self.show_message_panel("ReaLearn", msg, || {
            App::stop_learning_targets();
        });
        self.request_next_reaper_targets()
            .recv()
            .await
            .map_err(|_| "stopped learning")
    }

    fn stop_learning_targets() {
        App::get()
            .control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::StopLearning)
            .unwrap();
    }

    fn request_next_reaper_targets(&self) -> async_channel::Receiver<ReaperTarget> {
        let (sender, receiver) = async_channel::bounded(500);
        self.control_surface_main_task_sender
            .try_send(RealearnControlSurfaceMainTask::StartLearningTargets(sender))
            .unwrap();
        receiver
    }

    fn start_learning_source_for_target(
        &self,
        compartment: MappingCompartment,
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
            .or_else(|| self.find_first_relevant_session());
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
        compartment: MappingCompartment,
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
        compartment: MappingCompartment,
        target: &ReaperTarget,
    ) -> Option<(SharedSession, SharedMapping)> {
        self.sessions.borrow().iter().find_map(|session| {
            let session = session.upgrade()?;
            let mapping = {
                let s = session.borrow();
                if s.context().project() != project {
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
            session.context().track().contains(&track)
        })
    }

    fn find_first_relevant_session(&self) -> Option<SharedSession> {
        self.find_first_session_in_project(Some(Reaper::get().current_project()))
            .or_else(|| self.find_first_session_in_project(None))
    }

    /// Project None means monitoring FX chain.
    fn find_first_session_in_project(&self, project: Option<Project>) -> Option<SharedSession> {
        self.find_session(|session| {
            let session = session.borrow();
            session.context().project() == project
        })
    }

    fn find_first_relevant_session_with_input_from(
        &self,
        real_source: &QualifiedRealSource,
    ) -> Option<SharedSession> {
        self.find_first_session_with_input_from(Some(Reaper::get().current_project()), real_source)
            .or_else(|| self.find_first_session_with_input_from(None, real_source))
    }

    fn find_first_session_with_input_from(
        &self,
        project: Option<Project>,
        real_source: &QualifiedRealSource,
    ) -> Option<SharedSession> {
        let input_descriptor = real_source.to_input_descriptor(false);
        self.find_session(|session| {
            let session = session.borrow();
            session.context().project() == project && session.receives_input_from(&input_descriptor)
        })
    }

    fn find_first_relevant_session_with_source(
        &self,
        compartment: MappingCompartment,
        real_source: &QualifiedRealSource,
    ) -> Option<(SharedSession, SharedMapping)> {
        self.find_first_session_with_source(
            Some(Reaper::get().current_project()),
            compartment,
            real_source,
        )
        .or_else(|| self.find_first_session_with_source(None, compartment, real_source))
    }

    fn find_first_session_with_source(
        &self,
        project: Option<Project>,
        compartment: MappingCompartment,
        real_source: &QualifiedRealSource,
    ) -> Option<(SharedSession, SharedMapping)> {
        self.sessions.borrow().iter().find_map(|session| {
            let session = session.upgrade()?;
            let mapping = {
                let s = session.borrow();
                if s.context().project() != project {
                    return None;
                }
                let input_descriptor = real_source.to_input_descriptor(true);
                if !s.receives_input_from(&input_descriptor) {
                    return None;
                }
                let unqualified_real_source = real_source.clone().into_unqualified_real_source();
                s.find_mapping_with_source(compartment, &unqualified_real_source)?
                    .clone()
            };
            Some((session, mapping))
        })
    }

    fn find_session(&self, predicate: impl FnMut(&SharedSession) -> bool) -> Option<SharedSession> {
        self.sessions
            .borrow()
            .iter()
            .filter_map(|s| s.upgrade())
            .find(predicate)
    }

    fn server_resource_dir_path() -> PathBuf {
        Self::helgoboss_resource_dir_path().join("Server")
    }

    fn notify_changed(&self) {
        self.changed_subject.borrow_mut().next(());
    }

    fn party_is_over(&self) -> impl UnitEvent {
        self.party_is_over_subject.clone()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.message_panel.close();
        self.party_is_over_subject.next(());
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    main: MainConfig,
}

impl AppConfig {
    pub fn load() -> Result<AppConfig, String> {
        let ini_content = fs::read_to_string(&Self::config_file_path())
            .map_err(|_| "couldn't read config file".to_string())?;
        let config = serde_ini::from_str(&ini_content).map_err(|e| format!("{:?}", e))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), &'static str> {
        let ini_content = serde_ini::to_string(self).map_err(|_| "couldn't serialize config")?;
        let config_file_path = Self::config_file_path();
        fs::create_dir_all(&config_file_path.parent().unwrap())
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

#[derive(Serialize, Deserialize)]
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
        default = "default_companion_web_app_url",
        skip_serializing_if = "is_default_companion_web_app_url"
    )]
    companion_web_app_url: String,
}

const DEFAULT_SERVER_HTTP_PORT: u16 = 39080;
const DEFAULT_SERVER_HTTPS_PORT: u16 = 39443;

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
            companion_web_app_url: default_companion_web_app_url(),
        }
    }
}

fn build_detailed_version() -> String {
    use crate::infrastructure::plugin::built_info::*;
    let dirty_mark = if GIT_DIRTY.contains(&true) {
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
        "Couldn't start ReaLearn projection server because {}",
        info
    ))
}

fn determine_module_base_address() -> Option<usize> {
    let hinstance = Reaper::get()
        .medium_reaper()
        .plugin_context()
        .h_instance()?;
    Some(hinstance.as_ptr() as usize)
}

#[derive(Clone)]
enum QualifiedRealSource {
    Midi(MidiInputDeviceId, MidiSource),
    Osc(OscDeviceId, OscSource),
}

impl QualifiedRealSource {
    fn to_input_descriptor(&self, ignore_midi_channel: bool) -> InputDescriptor {
        match self {
            QualifiedRealSource::Midi(dev_id, source) => InputDescriptor::Midi {
                device_id: *dev_id,
                channel: if ignore_midi_channel {
                    None
                } else {
                    source.channel()
                },
            },
            QualifiedRealSource::Osc(dev_id, _) => InputDescriptor::Osc { device_id: *dev_id },
        }
    }

    fn into_unqualified_real_source(self) -> RealSource {
        match self {
            QualifiedRealSource::Midi(_, s) => RealSource::Midi(s),
            QualifiedRealSource::Osc(_, s) => RealSource::Osc(s),
        }
    }
}

impl HookPostCommand for App {
    fn call(command_id: CommandId, _flag: i32) {
        App::get()
            .additional_feedback_event_sender
            .try_send(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                command_id,
            }))
            .unwrap();
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
            .try_send(AdditionalFeedbackEvent::ActionInvoked(ActionInvokedEvent {
                command_id,
            }))
            .unwrap();
    }
}
