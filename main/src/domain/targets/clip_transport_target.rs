use crate::domain::{
    clip_play_state_unit_value, format_value_as_on_off, interpret_current_clip_slot_value,
    transport_is_enabled_unit_value, BackboneState, Compartment, CompoundChangeEvent,
    ControlContext, ExtendedProcessorContext, HitInstructionReturnValue, MappingControlContext,
    RealTimeControlContext, RealTimeReaperTarget, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target, UnitValue};
use playtime_clip_engine::main::{ClipMatrixEvent, ClipSlotCoordinates, ClipTransportOptions};
use playtime_clip_engine::rt::{
    ClipChangedEvent, ColumnPlayClipOptions, QualifiedClipChangedEvent,
};
use realearn_api::schema::ClipTransportAction;
use reaper_high::Project;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedClipTransportTarget {
    pub slot: VirtualClipSlot,
    pub action: ClipTransportAction,
    pub options: ClipTransportOptions,
}

impl UnresolvedReaperTargetDef for UnresolvedClipTransportTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context.project_or_current_project();
        let target = ClipTransportTarget {
            project,
            basics: ClipTransportTargetBasics {
                slot_coordinates: self.slot.resolve(context, compartment)?,
                action: self.action,
                options: self.options,
            },
        };
        Ok(vec![ReaperTarget::ClipTransport(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualClipSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipTransportTarget {
    project: Project,
    basics: ClipTransportTargetBasics,
}

#[derive(Clone, Debug, PartialEq)]
struct ClipTransportTargetBasics {
    pub slot_coordinates: ClipSlotCoordinates,
    pub action: ClipTransportAction,
    pub options: ClipTransportOptions,
}

impl ClipTransportTargetBasics {
    fn play_options(&self) -> ColumnPlayClipOptions {
        ColumnPlayClipOptions {
            stop_column_if_slot_empty: self.options.stop_column_if_slot_empty,
        }
    }
}

const NOT_RECORDING_BECAUSE_NOT_ARMED: &str = "not recording because not armed";

impl RealearnTarget for ClipTransportTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        control_type_and_character(self.basics.action)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        use ClipTransportAction::*;
        let on = value.is_on();
        BackboneState::get().with_clip_matrix_mut(
            context.control_context.instance_state,
            |matrix| {
                match self.basics.action {
                    PlayStop => {
                        if on {
                            matrix.play_clip(
                                self.basics.slot_coordinates,
                                self.basics.play_options(),
                            )?;
                        } else {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    PlayPause => {
                        if on {
                            matrix.play_clip(
                                self.basics.slot_coordinates,
                                self.basics.play_options(),
                            )?;
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
                            if self.basics.options.record_only_if_track_armed
                                && !matrix.column_is_armed_for_recording(
                                    self.basics.slot_coordinates.column(),
                                )
                            {
                                return Err(NOT_RECORDING_BECAUSE_NOT_ARMED);
                            }
                            matrix.record_clip(self.basics.slot_coordinates)?;
                        } else {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    RecordPlayStop => {
                        if on {
                            let slot_is_empty = matrix
                                .slot(self.basics.slot_coordinates)
                                .ok_or("slot doesn't exist")?
                                .is_empty();
                            if slot_is_empty {
                                // Slot is empty.
                                if self.basics.options.record_only_if_track_armed {
                                    // Record only if armed.
                                    if matrix.column_is_armed_for_recording(
                                        self.basics.slot_coordinates.column(),
                                    ) {
                                        // Is armed, so record.
                                        matrix.record_clip(self.basics.slot_coordinates)?;
                                    } else if self.basics.options.stop_column_if_slot_empty {
                                        // Not armed but column stopping on empty slots enabled.
                                        // Since we already know that the slot is empty, we do
                                        // it explicitly without invoking play passing that option.
                                        matrix
                                            .stop_column(self.basics.slot_coordinates.column())?;
                                    } else {
                                        return Err(NOT_RECORDING_BECAUSE_NOT_ARMED);
                                    }
                                } else {
                                    // Definitely record.
                                    matrix.record_clip(self.basics.slot_coordinates)?;
                                }
                            } else {
                                // Slot is filled.
                                matrix.play_clip(
                                    self.basics.slot_coordinates,
                                    self.basics.play_options(),
                                )?;
                            }
                        } else {
                            matrix.stop_clip(self.basics.slot_coordinates)?;
                        }
                    }
                    Looped => {
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
                use ClipTransportAction::*;
                match event {
                    ClipChangedEvent::PlayState(new_state) => match self.basics.action {
                        PlayStop | PlayPause | Stop | Pause | RecordStop => {
                            let uv = clip_play_state_unit_value(self.basics.action, *new_state);
                            (true, Some(AbsoluteValue::Continuous(uv)))
                        }
                        _ => (false, None),
                    },
                    ClipChangedEvent::ClipLooped(new_state) => match self.basics.action {
                        Looped => (
                            true,
                            Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                                *new_state,
                            ))),
                        ),
                        _ => (false, None),
                    },
                    ClipChangedEvent::Removed => {
                        tracing_debug!("Reacting to clip-removed event");
                        (true, None)
                    }
                    _ => (false, None),
                }
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipTransport)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        use ClipTransportAction::*;
        if matches!(self.basics.action, RecordStop | RecordPlayStop | Looped) {
            // These are not for real-time usage.
            return None;
        }
        let t = RealTimeClipTransportTarget {
            project: self.project,
            basics: self.basics.clone(),
        };
        Some(RealTimeReaperTarget::ClipTransport(t))
    }

    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        match key {
            "slot_state.id" => BackboneState::get()
                .with_clip_matrix(context.instance_state, |matrix| {
                    let id_string = match matrix.clip_play_state(self.basics.slot_coordinates) {
                        Ok(s) => s.id_string(),
                        Err(_) => "empty",
                    };
                    Some(PropValue::Text(id_string.into()))
                })
                .ok()?,
            _ => None,
        }
    }
}

impl<'a> Target<'a> for ClipTransportTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let val = BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                use ClipTransportAction::*;
                let val = match self.basics.action {
                    PlayStop | PlayPause | Stop | Pause | RecordStop | RecordPlayStop => {
                        let play_state =
                            matrix.clip_play_state(self.basics.slot_coordinates).ok()?;
                        clip_play_state_unit_value(self.basics.action, play_state)
                    }
                    Looped => {
                        let is_looped = matrix.clip_looped(self.basics.slot_coordinates).ok()?;
                        transport_is_enabled_unit_value(is_looped)
                    }
                };
                Some(AbsoluteValue::Continuous(val))
            })
            .ok()?;
        interpret_current_clip_slot_value(val)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeClipTransportTarget {
    project: Project,
    basics: ClipTransportTargetBasics,
}

impl RealTimeClipTransportTarget {
    pub fn hit(
        &mut self,
        value: ControlValue,
        context: RealTimeControlContext,
    ) -> Result<(), &'static str> {
        use ClipTransportAction::*;
        let on = value.is_on();
        let matrix = context.clip_matrix()?;
        let matrix = matrix.lock();
        match self.basics.action {
            PlayStop => {
                if on {
                    matrix.play_clip(self.basics.slot_coordinates, self.basics.play_options())
                } else {
                    matrix.stop_clip(self.basics.slot_coordinates)
                }
            }
            PlayPause => {
                if on {
                    matrix.play_clip(self.basics.slot_coordinates, self.basics.play_options())
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
            RecordStop | RecordPlayStop => Err("record not supported for real-time target"),
            Looped => Err("setting looped not supported for real-time target"),
        }
    }
}

impl<'a> Target<'a> for RealTimeClipTransportTarget {
    type Context = RealTimeControlContext<'a>;

    fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
        use ClipTransportAction::*;
        let matrix = context.clip_matrix().ok()?;
        let matrix = matrix.lock();
        let column = matrix.column(self.basics.slot_coordinates.column()).ok()?;
        let column = column.lock();
        let slot = column.slot(self.basics.slot_coordinates.row()).ok()?;
        let clip = match slot.clip() {
            Ok(c) => c,
            Err(_) => return interpret_current_clip_slot_value(None),
        };
        let val = match self.basics.action {
            PlayStop | PlayPause | Stop | Pause | RecordStop | RecordPlayStop => {
                clip_play_state_unit_value(self.basics.action, clip.play_state())
            }
            Looped => transport_is_enabled_unit_value(clip.looped()),
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, _: RealTimeControlContext<'a>) -> ControlType {
        control_type_and_character(self.basics.action).0
    }
}

pub const CLIP_TRANSPORT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Invoke transport action",
    short_name: "Clip transport",
    supports_track: false,
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};

fn control_type_and_character(action: ClipTransportAction) -> (ControlType, TargetCharacter) {
    use ClipTransportAction::*;
    match action {
        // Retriggerable because we want to be able to retrigger play!
        PlayStop | PlayPause | RecordPlayStop => (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        ),
        Stop | Pause | RecordStop | Looped => {
            (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
        }
    }
}
