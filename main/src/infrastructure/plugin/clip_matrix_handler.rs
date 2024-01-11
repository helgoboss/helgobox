use crate::application::{SharedUnitModel, WeakUnitModel};
use crate::domain::{
    Compartment, InstanceId, QualifiedClipMatrixEvent, RealTimeInstanceTask, ReaperTarget,
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
            .mappings(Compartment::Main)
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
                let control_unit = ControlUnit {
                    id: ControlUnitId::new(unit_model.unit_id().into()),
                    // TODO-high CONTINUE Introduce unit naming, makes sense anyway
                    name: "".to_string(),
                    // TODO-high CONTINUE The palette color will be provided by the controller if
                    //  we use one. But things should also work without controllers (controllers
                    //  are only a mechanism on top of everything existing). So another part of
                    //  Helgobox should be responsible for keeping that color. It should be on unit
                    //  level. And that means it would make sense to put it in the unit (just like
                    //  the top-left corner). But where to persist it? Putting it into
                    //  CompartmentModelData as dedicated field would leak Playtime-specific stuff
                    //  in there. But: Putting it in custom_data, that sounds good!
                    //
                    // Take from unit and (or even only!) keep in custom main compartment data.
                    //  custom_data.playtime.control_unit.palette_color (number)
                    palette_color: None,
                    top_left_corner: unit_model.unit().borrow().control_unit_top_left_corner(),
                    // TODO-high CONTINUE Column and row count are NOT provided by the controller
                    // definition because how many columns an rows are available depends on the
                    // particular usage.
                    //
                    // Take from custom main compartment data.
                    //  custom_data.playtime.control_unit.column_count (number)
                    column_count: 8,
                    // TODO-high CONTINUE Take from custom main compartment data.
                    //  custom_data.playtime.control_unit.row_count (number)
                    row_count: 8,
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
        if learning_mapping_id.compartment != Compartment::Main {
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
                Compartment::Main,
                &reaper_target,
            );
        });
    }

    fn remove_mapping_by_target(&self, target: SimpleMappingTarget) {
        self.do_async_with_session(move |shared_session| {
            let mut session = shared_session.borrow_mut();
            let reaper_target = ReaperTarget::from_simple_target(target);
            session.remove_mapping_by_target(Compartment::Main, &reaper_target);
        });
    }
}
