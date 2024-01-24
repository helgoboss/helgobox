use crate::application::{SharedUnitModel, WeakUnitModel};
use crate::domain::{
    CompartmentKind, InstanceId, QualifiedClipMatrixEvent, RealTimeInstanceTask, ReaperTarget,
};
use crate::infrastructure::plugin::WeakInstanceShell;
use base::{Global, NamedChannelSender};
use playtime_api::runtime::{
    ControlUnit, ControlUnitId, SimpleMappingContainer, SimpleMappingTarget,
};

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct MatrixHandler {
    instance_id: InstanceId,
    audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    real_time_instance_task_sender:
        base::SenderToRealTimeThread<crate::domain::RealTimeInstanceTask>,
    event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
    weak_instance_shell: WeakInstanceShell,
    main_unit_model: WeakUnitModel,
}

#[cfg(feature = "playtime")]
impl MatrixHandler {
    pub fn new(
        instance_id: InstanceId,
        audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
        real_time_instance_task_sender: base::SenderToRealTimeThread<
            crate::domain::RealTimeInstanceTask,
        >,
        event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
        weak_instance_shell: WeakInstanceShell,
        main_unit_model: WeakUnitModel,
    ) -> Self {
        Self {
            instance_id,
            audio_hook_task_sender,
            real_time_instance_task_sender,
            event_sender,
            weak_instance_shell,
            main_unit_model,
        }
    }

    fn do_async_with_session(&self, f: impl FnOnce(SharedUnitModel) + 'static) {
        let session = self.main_unit_model.clone();
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(move || {
                let shared_session = session.upgrade().expect("session gone");
                f(shared_session);
            })
            .unwrap();
    }
}

#[cfg(feature = "playtime")]
impl playtime_clip_engine::base::ClipMatrixHandler for MatrixHandler {
    fn init_recording(&self, command: playtime_clip_engine::base::HandlerInitRecordingCommand) {
        use crate::domain::NormalAudioHookTask;
        use playtime_clip_engine::rt::audio_hook::ClipEngineAudioHookCommand;
        use playtime_clip_engine::rt::fx_hook::ClipEngineFxHookCommand;
        match command.create_specific_command() {
            playtime_clip_engine::base::SpecificInitRecordingCommand::HardwareInput(t) => {
                let playtime_command = ClipEngineAudioHookCommand::InitRecording(t);
                self.audio_hook_task_sender.send_complaining(
                    NormalAudioHookTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
            playtime_clip_engine::base::SpecificInitRecordingCommand::FxInput(t) => {
                let playtime_command = ClipEngineFxHookCommand::InitRecording(t);
                self.real_time_instance_task_sender.send_complaining(
                    RealTimeInstanceTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
        }
    }

    fn emit_event(&self, event: playtime_clip_engine::base::ClipMatrixEvent) {
        let event = QualifiedClipMatrixEvent {
            instance_id: self.instance_id,
            event,
        };
        self.event_sender.send_complaining(event);
    }

    fn get_simple_mappings(&self) -> SimpleMappingContainer {
        let session = self.main_unit_model.upgrade().expect("session gone");
        let session = session.borrow();
        let simple_mappings = session
            .mappings(CompartmentKind::Main)
            .filter_map(|m| m.borrow().get_simple_mapping());
        SimpleMappingContainer {
            mappings: simple_mappings.collect(),
        }
    }

    fn get_control_units(&self) -> Vec<ControlUnit> {
        let Some(instance_shell) = self.weak_instance_shell.upgrade() else {
            return vec![];
        };
        instance_shell
            .additional_unit_models()
            .into_iter()
            .filter_map(|unit_model| {
                let unit_model = unit_model.borrow();
                let unit = unit_model.unit().borrow();
                let control_unit = ControlUnit {
                    id: ControlUnitId::new(unit_model.unit_id().into()),
                    // TODO-medium CONTINUE Introduce unit naming, makes sense anyway
                    name: "".to_string(),
                    palette_color: unit.control_unit_palette_color(),
                    top_left_corner: unit.control_unit_top_left_corner(),
                    column_count: unit.control_unit_column_count(),
                    row_count: unit.control_unit_row_count(),
                };
                Some(control_unit)
            })
            .collect()
    }

    fn get_currently_learning_target(&self) -> Option<SimpleMappingTarget> {
        let shared_session = self.main_unit_model.upgrade().expect("session gone");
        let session = shared_session.borrow();
        let instance_state = session.unit();
        let instance_state = instance_state.borrow();
        let learning_mapping_id = instance_state.mapping_which_learns_source().get()?;
        if learning_mapping_id.compartment != CompartmentKind::Main {
            return None;
        }
        let mapping = session.find_mapping_by_qualified_id(learning_mapping_id)?;
        let target = mapping.borrow().target_model.simple_target();
        target
    }

    fn toggle_learn_source_by_target(&self, target: SimpleMappingTarget) {
        self.do_async_with_session(move |shared_session| {
            let mut session = shared_session.borrow_mut();
            let reaper_target = ReaperTarget::from_simple_target(target);
            session.toggle_learn_source_for_target(
                &shared_session,
                CompartmentKind::Main,
                &reaper_target,
            );
        });
    }

    fn remove_mapping_by_target(&self, target: SimpleMappingTarget) {
        self.do_async_with_session(move |shared_session| {
            let mut session = shared_session.borrow_mut();
            let reaper_target = ReaperTarget::from_simple_target(target);
            session.remove_mapping_by_target(CompartmentKind::Main, &reaper_target);
        });
    }
}
