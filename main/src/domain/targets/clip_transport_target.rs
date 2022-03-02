use crate::domain::{
    clip_play_state_unit_value, format_value_as_on_off, transport_is_enabled_unit_value,
    BackboneState, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    HitInstructionReturnValue, MappingCompartment, MappingControlContext, RealTimeControlContext,
    RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, TransportAction, UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use playtime_clip_engine::main::{
    ClipMatrixEvent, ClipRecordTiming, ClipSlotCoordinates, RecordArgs, RecordKind, SlotPlayOptions,
};
use playtime_clip_engine::rt::{ClipChangedEvent, QualifiedClipChangedEvent};
use playtime_clip_engine::{clip_timeline, Timeline};
use reaper_high::Project;

#[derive(Debug)]
pub struct UnresolvedClipTransportTarget {
    pub slot: VirtualClipSlot,
    pub action: TransportAction,
    pub play_options: SlotPlayOptions,
}

impl UnresolvedReaperTargetDef for UnresolvedClipTransportTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context.project_or_current_project();
        let basics = ClipTransportTargetBasics {
            slot_coordinates: self.slot.resolve(context, compartment)?,
            action: self.action,
            play_options: self.play_options,
        };
        let targets = vec![ReaperTarget::ClipTransport(ClipTransportTarget {
            project,
            basics,
        })];
        Ok(targets)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipTransportTarget {
    pub project: Project,
    pub basics: ClipTransportTargetBasics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipTransportTargetBasics {
    pub slot_coordinates: ClipSlotCoordinates,
    pub action: TransportAction,
    pub play_options: SlotPlayOptions,
}

impl RealearnTarget for ClipTransportTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        self.basics.action.control_type_and_character()
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        use TransportAction::*;
        let on = value.is_on();
        BackboneState::get().with_clip_matrix_mut(
            context.control_context.instance_state,
            |matrix| {
                match self.basics.action {
                    PlayStop => {
                        if on {
                            matrix.play_clip(self.basics.slot_coordinates)?;
                        } else {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    PlayPause => {
                        if on {
                            matrix.play_clip(self.basics.slot_coordinates)?;
                        } else {
                            matrix.pause_clip_legacy(self.basics.slot_coordinates)?;
                        }
                    }
                    Stop => {
                        if on {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    Pause => {
                        if on {
                            matrix.pause_clip_legacy(self.basics.slot_coordinates)?;
                        }
                    }
                    RecordStop => {
                        if on {
                            let timing = {
                                let timeline = clip_timeline(Some(self.project), false);
                                let next_bar = timeline.next_bar_at(timeline.cursor_pos());
                                ClipRecordTiming::StartOnBarStopOnDemand {
                                    start_bar: next_bar,
                                }
                            };
                            matrix.record_clip_legacy(
                                self.basics.slot_coordinates,
                                RecordArgs {
                                    kind: RecordKind::Normal {
                                        play_after: true,
                                        timing,
                                        detect_downbeat: true,
                                    },
                                },
                            )?;
                        } else {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    Repeat => {
                        matrix.toggle_looped(self.basics.slot_coordinates)?;
                    }
                };
                Ok(None)
            },
        )?
    }

    fn is_available(&self, _: ControlContext) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::AllClipsChanged) => (true, None),
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                QualifiedClipChangedEvent {
                    slot_coordinates: sc,
                    event,
                },
            )) if *sc == self.basics.slot_coordinates => {
                use TransportAction::*;
                match self.basics.action {
                    PlayStop | PlayPause | Stop | Pause | RecordStop => match event {
                        ClipChangedEvent::PlayState(new_state) => {
                            let uv = clip_play_state_unit_value(self.basics.action, *new_state);
                            (true, Some(AbsoluteValue::Continuous(uv)))
                        }
                        _ => (false, None),
                    },
                    Repeat => match event {
                        ClipChangedEvent::ClipLooped(new_state) => (
                            true,
                            Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                                *new_state,
                            ))),
                        ),
                        _ => (false, None),
                    },
                }
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipTransport)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        use TransportAction::*;
        if matches!(self.basics.action, RecordStop | Repeat) {
            // These are not for real-time usage.
            return None;
        }
        let t = RealTimeClipTransportTarget {
            project: self.project,
            basics: self.basics.clone(),
        };
        Some(RealTimeReaperTarget::ClipTransport(t))
    }
}

impl<'a> Target<'a> for ClipTransportTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                use TransportAction::*;
                let val = match self.basics.action {
                    PlayStop | PlayPause | Stop | Pause | RecordStop => {
                        let play_state = matrix.clip_play_state(self.basics.slot_coordinates)?;
                        clip_play_state_unit_value(self.basics.action, play_state)
                    }
                    Repeat => {
                        let is_looped = matrix.clip_repeated(self.basics.slot_coordinates)?;
                        transport_is_enabled_unit_value(is_looped)
                    }
                };
                Some(AbsoluteValue::Continuous(val))
            })
            .ok()?
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipTransportTarget {
    pub project: Project,
    pub basics: ClipTransportTargetBasics,
}

impl RealTimeClipTransportTarget {
    pub fn hit(
        &mut self,
        value: ControlValue,
        context: RealTimeControlContext,
    ) -> Result<(), &'static str> {
        use TransportAction::*;
        let on = value.is_on();
        let matrix = context.clip_matrix()?;
        let matrix = matrix.lock();
        match self.basics.action {
            PlayStop => {
                if on {
                    matrix.play_clip(self.basics.slot_coordinates)
                } else {
                    matrix.stop_clip(self.basics.slot_coordinates)
                }
            }
            PlayPause => {
                if on {
                    matrix.play_clip(self.basics.slot_coordinates)
                } else {
                    matrix.pause_clip(self.basics.slot_coordinates)
                }
            }
            Stop => {
                if on {
                    matrix.stop_clip(self.basics.slot_coordinates)
                } else {
                    Ok(())
                }
            }
            Pause => {
                if on {
                    matrix.pause_clip(self.basics.slot_coordinates)
                } else {
                    Ok(())
                }
            }
            RecordStop => Err("record not supported for real-time target"),
            Repeat => Err("setting repeated not supported for real-time target"),
        }
    }
}

impl<'a> Target<'a> for RealTimeClipTransportTarget {
    type Context = RealTimeControlContext<'a>;

    fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
        let matrix = context.clip_matrix().ok()?;
        let matrix = matrix.lock();
        let column = matrix.column(self.basics.slot_coordinates.column()).ok()?;
        let column = column.lock();
        let clip = column
            .slot(self.basics.slot_coordinates.row())
            .ok()?
            .clip()
            .ok()?;
        use TransportAction::*;
        let val = match self.basics.action {
            PlayStop | PlayPause | Stop | Pause | RecordStop => {
                clip_play_state_unit_value(self.basics.action, clip.play_state())
            }
            Repeat => transport_is_enabled_unit_value(clip.looped()),
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
        self.basics.action.control_type_and_character().0
    }
}

pub const CLIP_TRANSPORT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Invoke transport action",
    short_name: "Clip transport",
    supports_track: false,
    supports_slot: true,
    ..DEFAULT_TARGET
};
