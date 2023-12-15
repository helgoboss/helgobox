use crate::application::WeakSession;
use crate::domain::{BackboneState, InstanceId, InstanceState, QualifiedClipMatrixEvent};
use base::NamedChannelSender;
use playtime_api::runtime;
use playtime_api::runtime::{
    NoteSource, SimpleMapping, SimpleMappingContainer, SimpleMappingTarget, SimpleSource,
};

#[cfg(feature = "playtime")]
pub fn get_or_insert_owned_clip_matrix(
    session: WeakSession,
    instance_state: &mut InstanceState,
) -> &mut playtime_clip_engine::base::Matrix {
    BackboneState::get().get_or_insert_owned_clip_matrix_from_instance_state(
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
        }
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
        SimpleMappingContainer {
            mappings: vec![SimpleMapping {
                source: SimpleSource::Note(NoteSource {
                    channel: 0,
                    number: 50,
                }),
                target: SimpleMappingTarget::TriggerSlot(runtime::SlotAddress {
                    column_index: 1,
                    row_index: 1,
                }),
            }],
        }
    }

    fn get_learning_target(&self) -> Option<SimpleMappingTarget> {
        Some(SimpleMappingTarget::TriggerSlot(runtime::SlotAddress {
            column_index: 3,
            row_index: 3,
        }))
    }

    fn toggle_learn_target(&self, target: SimpleMappingTarget) {
        todo!()
    }
}
