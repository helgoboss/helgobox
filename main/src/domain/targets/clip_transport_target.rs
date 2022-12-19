use crate::domain::{
    clip_play_state_unit_value, format_value_as_on_off, interpret_current_clip_slot_value,
    transport_is_enabled_unit_value, BackboneState, Compartment, CompoundChangeEvent,
    ControlContext, ExtendedProcessorContext, HitResponse, MappingControlContext,
    RealTimeControlContext, RealTimeReaperTarget, RealearnClipMatrix, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipSlot,
    DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target, UnitValue};
use playtime_clip_engine::base::{ClipMatrixEvent, ClipSlotAddress, ClipTransportOptions};
use playtime_clip_engine::rt::{
    ClipChangeEvent, ColumnPlayClipOptions, InternalClipPlayState, QualifiedClipChangeEvent,
    QualifiedSlotChangeEvent, SlotChangeEvent,
};
use realearn_api::persistence::ClipTransportAction;
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

impl ClipTransportTarget {
    pub fn clip_play_state(&self, matrix: &RealearnClipMatrix) -> Option<InternalClipPlayState> {
        matrix
            .find_slot(self.basics.slot_coordinates)?
            .play_state()
            .ok()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ClipTransportTargetBasics {
    pub slot_coordinates: ClipSlotAddress,
    pub action: ClipTransportAction,
    pub options: ClipTransportOptions,
}

impl ClipTransportTargetBasics {
    fn play_options(&self) -> ColumnPlayClipOptions {
        ColumnPlayClipOptions {
            stop_column_if_slot_empty: self.options.stop_column_if_slot_empty,
            start_timing: self.options.play_start_timing,
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
    ) -> Result<HitResponse, &'static str> {
        use ClipTransportAction::*;
        let on = value.is_on();
        BackboneState::get().with_clip_matrix_mut(
            context.control_context.instance_state,
            |matrix| {
                let response = match self.basics.action {
                    PlayStop => {
                        if on {
                            matrix.play_slot(
                                self.basics.slot_coordinates,
                                self.basics.play_options(),
                            )?;
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_coordinates,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    PlayPause => {
                        if on {
                            matrix.play_slot(
                                self.basics.slot_coordinates,
                                self.basics.play_options(),
                            )?;
                        } else {
                            matrix.pause_clip(self.basics.slot_coordinates)?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    Stop => {
                        if on {
                            matrix.stop_slot(
                                self.basics.slot_coordinates,
                                self.basics.options.play_stop_timing,
                            )?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                    Pause => {
                        if on {
                            matrix.pause_clip(self.basics.slot_coordinates)?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
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
                            matrix.record_slot(self.basics.slot_coordinates)?;
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_coordinates,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    RecordPlayStop => {
                        if on {
                            if matrix.slot_is_empty(self.basics.slot_coordinates) {
                                // Slot is empty.
                                if self.basics.options.record_only_if_track_armed {
                                    // Record only if armed.
                                    if matrix.column_is_armed_for_recording(
                                        self.basics.slot_coordinates.column(),
                                    ) {
                                        // Is armed, so record.
                                        matrix.record_slot(self.basics.slot_coordinates)?;
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
                                    matrix.record_slot(self.basics.slot_coordinates)?;
                                }
                            } else {
                                // Slot is filled.
                                matrix.play_slot(
                                    self.basics.slot_coordinates,
                                    self.basics.play_options(),
                                )?;
                            }
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_coordinates,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    Looped => {
                        if on {
                            matrix.toggle_looped(self.basics.slot_coordinates)?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                };
                Ok(response)
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
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::EverythingChanged) => (true, None),
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                QualifiedSlotChangeEvent {
                    slot_address: sc,
                    event,
                },
            )) if *sc == self.basics.slot_coordinates => {
                use ClipTransportAction::*;
                match event {
                    SlotChangeEvent::PlayState(new_state) => match self.basics.action {
                        PlayStop | PlayPause | Stop | Pause | RecordStop | RecordPlayStop => {
                            let uv = clip_play_state_unit_value(self.basics.action, *new_state);
                            (true, Some(AbsoluteValue::Continuous(uv)))
                        }
                        _ => (false, None),
                    },
                    SlotChangeEvent::Clips(_) => (true, None),
                    _ => (false, None),
                }
            }
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                QualifiedClipChangeEvent {
                    clip_address,
                    event,
                },
            )) if clip_address.slot_address == self.basics.slot_coordinates => {
                use ClipTransportAction::*;
                match event {
                    ClipChangeEvent::Looped(new_state) => match self.basics.action {
                        Looped => (
                            true,
                            Some(AbsoluteValue::Continuous(transport_is_enabled_unit_value(
                                *new_state,
                            ))),
                        ),
                        _ => (false, None),
                    },
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
                    let id_string = match self.clip_play_state(matrix) {
                        None => "empty",
                        Some(s) => s.id_string(),
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
                        let play_state = self.clip_play_state(matrix)?;
                        clip_play_state_unit_value(self.basics.action, play_state)
                    }
                    Looped => {
                        let is_looped = matrix
                            .find_slot(self.basics.slot_coordinates)?
                            .looped()
                            .ok()?;
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
                    matrix.stop_clip(
                        self.basics.slot_coordinates,
                        self.basics.options.play_stop_timing,
                    )
                }
            }
            PlayPause => {
                if on {
                    matrix.play_clip(self.basics.slot_coordinates, self.basics.play_options())
                } else {
                    matrix.pause_slot(self.basics.slot_coordinates)
                }
            }
            Stop => {
                if on {
                    matrix.stop_clip(
                        self.basics.slot_coordinates,
                        self.basics.options.play_stop_timing,
                    )
                } else {
                    Ok(())
                }
            }
            Pause => {
                if on {
                    matrix.pause_slot(self.basics.slot_coordinates)
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
        // Performance hint: This *will* sometimes be blocking IF we have live FX multiprocessing
        // turned on in REAPER. That's because then multiple threads will prepare stuff for the
        // main audio callback. According to Justin this is not a big deal though (given that we
        // lock only very shortly), so we allow blocking here. The only other alternative would be
        // to not acquire the value at this point and design the real-time clip transport target
        // in a way that the glue section doesn't need to know the current target value, leaving
        // the glue section powerless and making things such as "Toggle button" not work. This
        // would hurt the usual ReaLearn experience.
        let column = column.lock_allow_blocking();
        let slot = column.slot(self.basics.slot_coordinates.row()).ok()?;
        let Some(first_clip) = slot.clips().first() else {
            return Some(AbsoluteValue::Continuous(UnitValue::MIN));
        };
        let val = match self.basics.action {
            PlayStop | PlayPause | Stop | Pause | RecordStop | RecordPlayStop => {
                clip_play_state_unit_value(self.basics.action, first_clip.play_state())
            }
            Looped => transport_is_enabled_unit_value(first_clip.looped()),
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
    supports_real_time_control: true,
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
