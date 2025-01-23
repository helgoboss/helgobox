use crate::application::{SharedUnitModel, WeakUnitModel};
use crate::domain::{
    CompartmentKind, InstanceId, PlaytimeColumnActionTarget, PlaytimeMatrixActionTarget,
    PlaytimeRowActionTarget, PlaytimeSlotTransportTarget, QualifiedClipMatrixEvent,
    RealTimeInstanceTask, ReaperTarget,
};
use crate::infrastructure::plugin::WeakInstanceShell;
use anyhow::Context;
use base::{spawn_in_main_thread, Global, NamedChannelSender};
use helgobox_api::persistence::{
    PlaytimeColumnAction, PlaytimeMatrixAction, PlaytimeRowAction, PlaytimeSlotTransportAction,
};
use playtime_api::runtime::{
    ControlUnit, ControlUnitId, SimpleMappingContainer, SimpleMappingTarget,
};
use playtime_clip_engine::base::ClipMatrixEvent;
use reaper_high::Reaper;

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

    fn invalidate_control_units(&self) {
        let weak_instance_shell = self.weak_instance_shell.clone();
        spawn_in_main_thread(async move {
            let instance_shell = weak_instance_shell
                .upgrade()
                .context("instance shell gone")?;
            let instance = instance_shell.instance().borrow();
            let matrix = instance.clip_matrix().context("no matrix")?;
            let column_count = matrix.column_count();
            let row_count = matrix.row_count();
            for unit_model in instance_shell.additional_unit_models() {
                unit_model
                    .borrow()
                    .unit()
                    .borrow_mut()
                    .invalidate_control_unit_scroll_pos(column_count, row_count);
            }
            Ok(())
        });
    }

    fn do_async_with_main_unit_model(&self, f: impl FnOnce(SharedUnitModel) + 'static) {
        let session = self.main_unit_model.clone();
        Global::task_support()
            .do_later_in_main_thread_from_main_thread_asap(move || {
                let shared_session = session.upgrade().expect("session gone");
                f(shared_session);
            })
            .unwrap();
    }
}

impl playtime_clip_engine::base::ClipMatrixHandler for MatrixHandler {
    fn init_recording(&self, command: playtime_clip_engine::base::HandlerInitRecordingCommand) {
        use crate::domain::NormalAudioHookTask;
        use playtime_clip_engine::rt::audio_hook::PlaytimeAudioHookCommand;
        use playtime_clip_engine::rt::fx_hook::PlaytimeFxHookCommand;
        match command.create_specific_command() {
            playtime_clip_engine::base::SpecificInitRecordingCommand::HardwareInput(t) => {
                let playtime_command = PlaytimeAudioHookCommand::InitRecording(t);
                self.audio_hook_task_sender.send_complaining(
                    NormalAudioHookTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
            playtime_clip_engine::base::SpecificInitRecordingCommand::FxInput(t) => {
                let playtime_command = PlaytimeFxHookCommand::InitRecording(t);
                self.real_time_instance_task_sender.send_complaining(
                    RealTimeInstanceTask::PlaytimeClipEngineCommand(playtime_command),
                );
            }
        }
    }

    fn emit_event(&self, event: playtime_clip_engine::base::ClipMatrixEvent) {
        // Check if we should do something special with that event
        if matches!(event, ClipMatrixEvent::EverythingChanged) {
            self.invalidate_control_units();
        }
        // Send event to receiver
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
            .map(|unit_model| {
                let unit_model = unit_model.borrow();
                let unit = unit_model.unit().borrow();
                ControlUnit {
                    id: ControlUnitId::new(unit_model.unit_id().into()),
                    name: unit_model.name_or_key().to_string(),
                    palette_color: unit.control_unit_palette_color(),
                    top_left_corner: unit.control_unit_top_left_corner(),
                    column_count: unit.control_unit_column_count(),
                    row_count: unit.control_unit_row_count(),
                }
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
        self.do_async_with_main_unit_model(move |shared_session| {
            let mut session = shared_session.borrow_mut();
            let mapping_desc = SimpleMappingDesc::from_simple_target(target);
            session.toggle_learn_source_for_target(
                &shared_session,
                CompartmentKind::Main,
                &mapping_desc.reaper_target,
                mapping_desc.toggle,
            );
        });
    }

    fn remove_mapping_by_target(&self, target: SimpleMappingTarget) {
        self.do_async_with_main_unit_model(move |shared_session| {
            let mut session = shared_session.borrow_mut();
            let mapping_desc = SimpleMappingDesc::from_simple_target(target);
            session.remove_mapping_by_target(CompartmentKind::Main, &mapping_desc.reaper_target);
        });
    }

    fn warn_about_less_important_issue(&self, message: &str) {
        // The "!SHOW:" prefix makes the console window not open since REAPER 7.
        Reaper::get().show_console_msg(format!("!SHOW:Helgobox: {message}"));
    }
}

struct SimpleMappingDesc {
    pub reaper_target: ReaperTarget,
    pub toggle: bool,
}

impl SimpleMappingDesc {
    pub fn from_simple_target(simple_target: SimpleMappingTarget) -> Self {
        use SimpleMappingTarget::*;
        match simple_target {
            TriggerMatrix => Self {
                reaper_target: ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                    action: PlaytimeMatrixAction::Stop,
                }),
                toggle: false,
            },
            TriggerColumn(t) => Self {
                reaper_target: ReaperTarget::PlaytimeColumnAction(PlaytimeColumnActionTarget {
                    column_index: t.index,
                    action: PlaytimeColumnAction::Stop,
                }),
                toggle: false,
            },
            TriggerRow(t) => Self {
                reaper_target: ReaperTarget::PlaytimeRowAction(PlaytimeRowActionTarget {
                    basics: crate::domain::ClipRowTargetBasics {
                        row_index: t.index,
                        action: PlaytimeRowAction::PlayScene,
                    },
                }),
                toggle: false,
            },
            TriggerSlot(t) => Self {
                reaper_target: ReaperTarget::PlaytimeSlotTransportAction(
                    PlaytimeSlotTransportTarget {
                        project: Reaper::get().current_project(),
                        basics: crate::domain::ClipTransportTargetBasics {
                            slot_address: t,
                            action: PlaytimeSlotTransportAction::Trigger,
                            options: Default::default(),
                        },
                    },
                ),
                toggle: false,
            },
            SmartRecord => Self {
                reaper_target: ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                    action: PlaytimeMatrixAction::SmartRecord,
                }),
                toggle: false,
            },
            EnterSilenceModeOrPlayIgnited => Self {
                reaper_target: {
                    ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                        action: PlaytimeMatrixAction::StartOrStopPlayback,
                    })
                },
                toggle: true,
            },
            SequencerRecordOnOffState => Self {
                reaper_target: ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                    action: PlaytimeMatrixAction::SequencerRecordOnOffState,
                }),
                toggle: true,
            },
            SequencerPlayOnOffState => Self {
                reaper_target: ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                    action: PlaytimeMatrixAction::SequencerPlayOnOffState,
                }),
                toggle: true,
            },
            TapTempo => Self {
                reaper_target: ReaperTarget::PlaytimeMatrixAction(PlaytimeMatrixActionTarget {
                    action: PlaytimeMatrixAction::TapTempo,
                }),
                toggle: false,
            },
        }
    }
}
