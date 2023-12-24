use vst::plugin::HostCallback;

use crate::domain::{
    AudioBlockProps, Backbone, ControlEvent, IncomingMidiMessage, MainProcessor, MidiEvent,
    ParameterMainTask, PluginParamIndex, PluginParams, ProcessorContext, RawParamValue,
    RealTimeProcessorLocker, SharedRealTimeProcessor, UnitId,
};
use crate::domain::{NormalRealTimeTask, RealTimeProcessor};
use crate::infrastructure::plugin::{InstanceParamContainer, PluginInstanceInfo};
use crate::infrastructure::ui::UnitPanel;
use base::{NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use reaper_medium::{Hz, ProjectRef};

use slog::{debug, o};
use std::cell::RefCell;

use std::rc::Rc;

use fragile::Fragile;
use reaper_high::Reaper;
use std::sync::{Arc, Mutex};
use swell_ui::SharedView;

use crate::application::{InstanceModel, SharedInstanceModel};
use crate::infrastructure::plugin::backbone_shell::BackboneShell;

use crate::base::notification;
use crate::infrastructure::data::SessionData;
use crate::infrastructure::server::http::keep_informing_clients_about_session_events;
use crate::infrastructure::ui::instance_panel::InstancePanel;

const NORMAL_REAL_TIME_TASK_QUEUE_SIZE: usize = 1000;
const FEEDBACK_REAL_TIME_TASK_QUEUE_SIZE: usize = 2000;
const NORMAL_REAL_TIME_TO_MAIN_TASK_QUEUE_SIZE: usize = 10_000;
const CONTROL_MAIN_TASK_QUEUE_SIZE: usize = 5000;
const PARAMETER_MAIN_TASK_QUEUE_SIZE: usize = 5000;

#[derive(Debug)]
pub struct UnitShell {
    /// An ID which is randomly generated on each start and is most relevant for log correlation.
    /// It's also used in other ReaLearn singletons. Must be unique.
    instance_id: UnitId,
    logger: slog::Logger,
    /// Fragile because we can't access this from any other thread than the main thread.
    session: Fragile<SharedInstanceModel>,
    normal_real_time_task_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    parameter_main_task_sender: SenderToNormalThread<ParameterMainTask>,
    // Called in real-time audio thread only.
    // We keep it in this struct in order to be able to inform it about incoming FX MIDI messages
    // and drive its processing without detour. Well, almost. We share it with the global ReaLearn
    // audio hook that also drives processing (because in some cases the VST processing is
    // stopped). That's why we need an Rc/RefCell.
    real_time_processor: SharedRealTimeProcessor,
}

impl UnitShell {
    pub fn new(
        processor_context: ProcessorContext,
        unit_parameter_container: Arc<InstanceParamContainer>,
        unit_panel: SharedView<InstancePanel>,
    ) -> Self {
        let (normal_real_time_task_sender, normal_real_time_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "normal real-time tasks",
                NORMAL_REAL_TIME_TASK_QUEUE_SIZE,
            );
        let (feedback_real_time_task_sender, feedback_real_time_task_receiver) =
            SenderToRealTimeThread::new_channel(
                "feedback real-time tasks",
                FEEDBACK_REAL_TIME_TASK_QUEUE_SIZE,
            );
        let (normal_main_task_sender, normal_main_task_receiver) =
            SenderToNormalThread::new_unbounded_channel("normal main tasks");
        let (normal_rt_to_main_task_sender, normal_rt_to_main_task_receiver) =
            SenderToNormalThread::new_bounded_channel(
                "normal real-time to main tasks",
                NORMAL_REAL_TIME_TO_MAIN_TASK_QUEUE_SIZE,
            );
        let (control_main_task_sender, control_main_task_receiver) =
            SenderToNormalThread::new_bounded_channel(
                "control main tasks",
                CONTROL_MAIN_TASK_QUEUE_SIZE,
            );
        let (parameter_main_task_sender, parameter_main_task_receiver) =
            SenderToNormalThread::new_bounded_channel(
                "parameter main tasks",
                PARAMETER_MAIN_TASK_QUEUE_SIZE,
            );
        let instance_id = UnitId::next();
        let logger = BackboneShell::logger().new(o!("instance" => instance_id.to_string()));
        let real_time_processor = RealTimeProcessor::new(
            instance_id,
            &logger,
            normal_real_time_task_receiver,
            feedback_real_time_task_receiver,
            feedback_real_time_task_sender.clone(),
            normal_rt_to_main_task_sender,
            control_main_task_sender,
        );
        let real_time_processor = Arc::new(Mutex::new(real_time_processor));
        // This is necessary since Rust 1.62.0 (or 1.63.0, not sure). Since those versions,
        // locking a mutex the first time apparently allocates. If we don't lock the
        // mutex now for the first time but do it in the real-time thread, assert_no_alloc will
        // complain in debug builds.
        drop(real_time_processor.lock_recover());
        // Start create session
        // Instance state (domain - shared)
        let (instance_feedback_event_sender, instance_feedback_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("instance state change events");
        let instance_state = Backbone::get().create_instance(
            instance_id,
            processor_context.clone(),
            instance_feedback_event_sender,
            #[cfg(feature = "playtime")]
            BackboneShell::get().clip_matrix_event_sender().clone(),
            #[cfg(feature = "playtime")]
            BackboneShell::get().normal_audio_hook_task_sender().clone(),
            #[cfg(feature = "playtime")]
            normal_real_time_task_sender.clone(),
        );
        // Session (application - shared)
        let session = InstanceModel::new(
            instance_id,
            &logger,
            processor_context.clone(),
            normal_real_time_task_sender.clone(),
            normal_main_task_sender.clone(),
            unit_parameter_container.clone(),
            BackboneShell::get(),
            BackboneShell::get().controller_preset_manager(),
            BackboneShell::get().main_preset_manager(),
            BackboneShell::get().preset_link_manager(),
            instance_state.clone(),
            BackboneShell::get().feedback_audio_hook_task_sender(),
            feedback_real_time_task_sender.clone(),
            BackboneShell::get().osc_feedback_task_sender(),
            BackboneShell::get().control_surface_main_task_sender(),
        );
        let shared_session = Rc::new(RefCell::new(session));
        let weak_session = Rc::downgrade(&shared_session);
        let main_panel = UnitPanel::new(
            weak_session.clone(),
            Arc::downgrade(&unit_parameter_container),
        );
        shared_session
            .borrow_mut()
            .set_ui(Rc::downgrade(&main_panel));
        keep_informing_clients_about_session_events(&shared_session);
        let plugin_instance_info = PluginInstanceInfo {
            processor_context: processor_context.clone(),
            instance_id,
            instance_state: Rc::downgrade(&instance_state),
            session: weak_session.clone(),
            ui: Rc::downgrade(&main_panel),
        };
        BackboneShell::get().register_plugin_instance(plugin_instance_info);
        // Main processor - (domain, owned by REAPER control surface)
        // Register the main processor with the global ReaLearn control surface. We let it
        // call by the control surface because it must be called regularly,
        // even when the ReaLearn UI is closed. That means, the VST GUI idle
        // callback is not suited.
        let main_processor = MainProcessor::new(
            instance_id,
            &logger,
            normal_main_task_sender.clone(),
            normal_main_task_receiver,
            normal_rt_to_main_task_receiver,
            parameter_main_task_receiver,
            control_main_task_receiver,
            instance_feedback_event_receiver,
            normal_real_time_task_sender.clone(),
            feedback_real_time_task_sender,
            BackboneShell::get()
                .feedback_audio_hook_task_sender()
                .clone(),
            BackboneShell::get().additional_feedback_event_sender(),
            BackboneShell::get().instance_orchestration_event_sender(),
            BackboneShell::get().osc_feedback_task_sender().clone(),
            weak_session.clone(),
            processor_context,
            instance_state,
            BackboneShell::get(),
        );
        BackboneShell::get().register_processor_couple(
            instance_id,
            real_time_processor.clone(),
            main_processor,
        );
        shared_session.borrow_mut().activate(weak_session.clone());
        unit_panel.notify_main_unit_panel_available(main_panel);
        shared_session.borrow().notify_realearn_instance_started();
        // End create session
        Self {
            instance_id,
            logger: logger.clone(),
            // InstanceShell is the main owner of the InstanceModel. Everywhere else the InstanceModel is
            // just temporarily upgraded, never stored as Rc, only as Weak.
            session: Fragile::new(shared_session),
            normal_real_time_task_sender,
            parameter_main_task_sender,
            real_time_processor,
        }
    }

    /// Panics if not called from main thread.
    pub fn model(&self) -> &SharedInstanceModel {
        self.session.get()
    }

    pub fn set_all_parameters(&self, params: PluginParams) {
        // Propagate
        // send_if_space because https://github.com/helgoboss/realearn/issues/847
        self.parameter_main_task_sender
            .send_if_space(ParameterMainTask::UpdateAllParams(params));
    }

    pub fn set_single_parameter(&self, index: PluginParamIndex, value: RawParamValue) {
        // We immediately send to the main processor. Sending to the session and using the
        // session parameter list as single source of truth is no option because this method
        // will be called in a processing thread, not in the main thread. Not even a mutex would
        // help here because the session is conceived for main-thread usage only! I was not
        // aware of this being called in another thread and it led to subtle errors of course
        // (https://github.com/helgoboss/realearn/issues/59).
        // When rendering, we don't do it because that will accumulate until the rendering is
        // finished, which is pointless.
        if is_rendering() {
            return;
        }
        self.parameter_main_task_sender
            .send_complaining(ParameterMainTask::UpdateSingleParamValue { index, value });
    }

    pub fn apply_session_data(&self, session_data: &SessionData) -> anyhow::Result<PluginParams> {
        let shared_session = self.session.get();
        let mut session = shared_session.borrow_mut();
        if let Some(v) = session_data.version.as_ref() {
            if BackboneShell::version() < v {
                notification::warn(format!(
                    "The session that is about to load was saved with ReaLearn {}, which is \
                         newer than the installed version {}. Things might not work as expected. \
                         Even more importantly: Saving might result in loss of the data that was \
                         saved with the new ReaLearn version! Please consider upgrading your \
                         ReaLearn installation to the latest version.",
                    v,
                    BackboneShell::version()
                ));
            }
        }
        let params = session_data.create_params();
        if let Err(e) =
            session_data.apply_to_model(&mut session, &params, Rc::downgrade(&shared_session))
        {
            notification::warn(e.to_string());
        }
        // Update parameters
        self.parameter_main_task_sender
            .send_complaining(ParameterMainTask::UpdateAllParams(params.clone()));
        // Notify
        session.notify_everything_has_changed();
        Ok(params)
    }

    pub fn process_incoming_midi_from_vst(
        &self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        is_transport_start: bool,
        host: HostCallback,
    ) {
        self.real_time_processor
            .lock_recover()
            .process_incoming_midi_from_vst(event, is_transport_start, &host);
    }

    pub fn run_from_vst(
        &self,
        #[cfg(feature = "playtime")] buffer: &mut vst::buffer::AudioBuffer<f64>,
        #[cfg(feature = "playtime")] block_props: AudioBlockProps,
        host: HostCallback,
    ) {
        self.real_time_processor
            .lock_recover()
            .run_from_vst(buffer, block_props, &host);
    }

    pub fn set_sample_rate(&self, rate: f32) {
        // This is called in main thread, so we need to send it to the real-time processor via
        // channel. Real-time processor needs sample rate to do some MIDI clock calculations.
        // If task queue is full or audio not running, so what. Don't spam the user with error
        // messages.
        self.normal_real_time_task_sender
            .send_if_space(NormalRealTimeTask::UpdateSampleRate(Hz::new(rate as _)));
    }
}

impl Drop for UnitShell {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping instance shell...");
        let session = self.session.get();
        BackboneShell::get().unregister_processor_couple(self.instance_id);
        BackboneShell::get().unregister_session(session.as_ptr());
        debug!(
            self.logger,
            "{} pointers are still referring to this session",
            Rc::strong_count(session)
        );
    }
}

fn is_rendering() -> bool {
    Reaper::get()
        .medium_reaper()
        .enum_projects(ProjectRef::CurrentlyRendering, 0)
        .is_some()
}
