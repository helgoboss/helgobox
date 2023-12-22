use crate::application::{SharedSession, WeakSession};
use crate::domain::{
    Backbone, Compartment, Instance, InstanceId, QualifiedClipMatrixEvent, ReaperTarget,
};
use base::{Global, NamedChannelSender};
use playtime_api::runtime::{SimpleMappingContainer, SimpleMappingTarget};

#[cfg(feature = "playtime")]
pub fn get_or_insert_owned_clip_matrix(
    session: WeakSession,
    instance_state: &mut Instance,
) -> &mut playtime_clip_engine::base::Matrix {
    Backbone::get().get_or_insert_owned_clip_matrix_from_instance_state(
        instance_state,
        move |instance_state| {
            let handler = MatrixHandler::new(
                instance_state.instance_id(),
                instance_state.audio_hook_task_sender.clone(),
                instance_state.real_time_processor_sender.clone(),
                instance_state.clip_matrix_event_sender.clone(),
                session,
            );
            Box::new(handler)
        },
    )
}

#[cfg(feature = "playtime")]
#[derive(Debug)]
pub struct MatrixHandler {
    instance_id: InstanceId,
    audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
    real_time_processor_sender: base::SenderToRealTimeThread<crate::domain::NormalRealTimeTask>,
    event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
    session: WeakSession,
}

#[cfg(feature = "playtime")]
impl MatrixHandler {
    pub fn new(
        instance_id: InstanceId,
        audio_hook_task_sender: base::SenderToRealTimeThread<crate::domain::NormalAudioHookTask>,
        real_time_processor_sender: base::SenderToRealTimeThread<crate::domain::NormalRealTimeTask>,
        event_sender: base::SenderToNormalThread<QualifiedClipMatrixEvent>,
        session: WeakSession,
    ) -> Self {
        Self {
            instance_id,
            audio_hook_task_sender,
            real_time_processor_sender,
            event_sender,
            session,
        }
    }

    fn do_async_with_session(&self, f: impl FnOnce(SharedSession) + 'static) {
        let session = self.session.clone();
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
        use crate::domain::{NormalAudioHookTask, NormalRealTimeTask};
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
                self.real_time_processor_sender.send_complaining(
                    NormalRealTimeTask::PlaytimeClipEngineCommand(playtime_command),
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
        let session = self.session.upgrade().expect("session gone");
        let session = session.borrow();
        let simple_mappings = session
            .mappings(Compartment::Main)
            .filter_map(|m| m.borrow().get_simple_mapping());
        SimpleMappingContainer {
            mappings: simple_mappings.collect(),
        }
    }

    fn get_currently_learning_target(&self) -> Option<SimpleMappingTarget> {
        let shared_session = self.session.upgrade().expect("session gone");
        let session = shared_session.borrow();
        let instance_state = session.instance_state();
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
