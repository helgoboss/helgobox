use vst::plugin::HostCallback;

use crate::domain::{
    ControlEvent, IncomingMidiMessage, InstanceId, MainProcessor, MidiEvent, ParameterManager,
    ProcessorContext, RealTimeProcessorLocker, SharedInstance, SharedRealTimeProcessor, Unit,
    UnitId, WeakRealTimeInstance,
};
use crate::domain::{NormalRealTimeTask, RealTimeProcessor};
use crate::infrastructure::plugin::UnitInfo;
use crate::infrastructure::ui::UnitPanel;
use base::{NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use reaper_medium::Hz;

use std::cell::{Cell, RefCell};

use std::rc::Rc;

use fragile::Fragile;
use std::sync::{Arc, Mutex};
use swell_ui::{SharedView, WeakView};
use tracing::debug;

use crate::application::{AutoUnitData, SharedUnitModel, UnitModel};
use crate::infrastructure::plugin::backbone_shell::BackboneShell;

use crate::infrastructure::data::UnitData;
use crate::infrastructure::server::http::keep_informing_clients_about_session_events;
use crate::infrastructure::ui::instance_panel::InstancePanel;

const NORMAL_REAL_TIME_TASK_QUEUE_SIZE: usize = 1000;
const FEEDBACK_REAL_TIME_TASK_QUEUE_SIZE: usize = 2000;
const NORMAL_REAL_TIME_TO_MAIN_TASK_QUEUE_SIZE: usize = 10_000;
const CONTROL_MAIN_TASK_QUEUE_SIZE: usize = 5000;
/// I increased this for https://github.com/helgoboss/helgobox/issues/913. More processed
/// OSC messages processed at once in combination with target "FX parameter: Set value"
/// caused this queue to explode.
const PARAMETER_MAIN_TASK_QUEUE_SIZE: usize = 20_000;

#[derive(Debug)]
pub struct UnitShell {
    /// An ID which is randomly generated on each start and is most relevant for log correlation.
    /// It's also used in other ReaLearn singletons. Must be unique.
    id: UnitId,
    /// Fragile because we can't access this from any other thread than the main thread.
    model: Fragile<SharedUnitModel>,
    panel: Fragile<SharedView<UnitPanel>>,
    normal_real_time_task_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    // Called in real-time audio thread only.
    // We keep it in this struct in order to be able to inform it about incoming FX MIDI messages
    // and drive its processing without detour. Well, almost. We share it with the global ReaLearn
    // audio hook that also drives processing (because in some cases the VST processing is
    // stopped). That's why we need an Rc/RefCell.
    real_time_processor: SharedRealTimeProcessor,
    notified_unit_about_start: Fragile<Cell<bool>>,
}

impl UnitShell {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        unit_id: UnitId,
        initial_name: Option<String>,
        instance_id: InstanceId,
        processor_context: ProcessorContext,
        parent_instance: SharedInstance,
        parent_rt_instance: WeakRealTimeInstance,
        instance_panel: WeakView<InstancePanel>,
        is_main_unit: bool,
        auto_unit: Option<AutoUnitData>,
    ) -> Self {
        let is_auto_unit = auto_unit.is_some();
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
        let real_time_processor = RealTimeProcessor::new(
            unit_id,
            parent_rt_instance,
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
        let (unit_feedback_event_sender, unit_feedback_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("unit state change events");
        let parameter_manager = ParameterManager::new(parameter_main_task_sender);
        let unit = Unit::new(
            unit_id,
            is_main_unit,
            Rc::downgrade(&parent_instance),
            unit_feedback_event_sender,
            parameter_manager,
        );
        let unit = Rc::new(RefCell::new(unit));
        // Session (application - shared)
        let unit_model = UnitModel::new(
            instance_id,
            unit_id,
            initial_name,
            processor_context.clone(),
            normal_main_task_sender.clone(),
            BackboneShell::get(),
            BackboneShell::get().controller_preset_manager().clone(),
            BackboneShell::get().main_preset_manager().clone(),
            BackboneShell::get().preset_link_manager(),
            parent_instance.clone(),
            unit.clone(),
            BackboneShell::get().feedback_audio_hook_task_sender(),
            feedback_real_time_task_sender.clone(),
            BackboneShell::get().osc_feedback_task_sender(),
            BackboneShell::get().control_surface_main_task_sender(),
            auto_unit,
        );
        let shared_unit_model = Rc::new(RefCell::new(unit_model));
        let weak_session = Rc::downgrade(&shared_unit_model);
        let unit_panel = UnitPanel::new(instance_id, weak_session.clone(), instance_panel.clone());
        shared_unit_model
            .borrow_mut()
            .set_ui(Rc::downgrade(&unit_panel));
        keep_informing_clients_about_session_events(&shared_unit_model);
        // Main processor - (domain, owned by REAPER control surface)
        // Register the main processor with the global ReaLearn control surface. We let it
        // call by the control surface because it must be called regularly,
        // even when the ReaLearn UI is closed. That means, the VST GUI idle
        // callback is not suited.
        let main_processor = MainProcessor::new(
            instance_id,
            unit_id,
            normal_main_task_sender.clone(),
            normal_main_task_receiver,
            normal_rt_to_main_task_receiver,
            parameter_main_task_receiver,
            control_main_task_receiver,
            unit_feedback_event_receiver,
            normal_real_time_task_sender.clone(),
            feedback_real_time_task_sender,
            BackboneShell::get()
                .feedback_audio_hook_task_sender()
                .clone(),
            BackboneShell::get().additional_feedback_event_sender(),
            BackboneShell::get().instance_orchestration_event_sender(),
            BackboneShell::get().osc_feedback_task_sender().clone(),
            weak_session.clone(),
            processor_context.clone(),
            parent_instance.clone(),
            unit.clone(),
            BackboneShell::get(),
        );
        // TODO-high-playtime-refactoring We should register this like the instance - one layer higher and only
        //  by passing the UnitShell (the root for everything unit-related).
        let unit_info = UnitInfo {
            unit_id,
            instance_id,
            instance: Rc::downgrade(&parent_instance),
            unit_model: weak_session.clone(),
            instance_panel,
            is_main_unit,
            unit: Rc::downgrade(&unit),
            is_auto_unit,
        };
        BackboneShell::get().register_unit(unit_info, real_time_processor.clone(), main_processor);
        shared_unit_model
            .borrow_mut()
            .activate(weak_session.clone());
        // End create session
        Self {
            id: unit_id,
            // InstanceShell is the main owner of the InstanceModel. Everywhere else the InstanceModel is
            // just temporarily upgraded, never stored as Rc, only as Weak.
            model: Fragile::new(shared_unit_model),
            panel: Fragile::new(unit_panel),
            normal_real_time_task_sender,
            real_time_processor,
            notified_unit_about_start: Default::default(),
        }
    }

    pub fn id(&self) -> UnitId {
        self.id
    }

    /// Panics if not called from main thread.
    pub fn model(&self) -> &SharedUnitModel {
        self.model.get()
    }

    /// Panics if not called from main thread.
    pub fn panel(&self) -> &SharedView<UnitPanel> {
        self.panel.get()
    }

    pub fn apply_data(&self, session_data: &UnitData) -> anyhow::Result<()> {
        let model = self.model.get();
        session_data.apply_to_model(model)?;
        if !self.notified_unit_about_start.get().replace(true) {
            model.borrow().notify_realearn_unit_started();
        }
        Ok(())
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

    pub fn run_from_vst(&self, host: HostCallback) {
        self.real_time_processor.lock_recover().run_from_vst(&host);
    }

    pub fn set_sample_rate(&self, rate: f32) {
        // This is called in main thread, so we need to send it to the real-time processor via
        // channel. Real-time processor needs sample rate to do some MIDI clock calculations.
        // If task queue is full or audio not running, so what. Don't spam the user with error
        // messages.
        self.normal_real_time_task_sender
            .send_if_space(NormalRealTimeTask::UpdateSampleRate(Hz::new_panic(
                rate as _,
            )));
    }
}

impl Drop for UnitShell {
    fn drop(&mut self) {
        debug!("Dropping UnitShell {}...", self.id);
        let session = self.model.get();
        if BackboneShell::is_loaded() {
            BackboneShell::get().unregister_unit(self.id);
        }
        debug!(
            "{} pointers are still referring to this session",
            Rc::strong_count(session)
        );
    }
}
