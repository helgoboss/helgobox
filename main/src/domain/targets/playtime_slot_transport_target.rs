use crate::domain::{
    CompartmentKind, ExtendedProcessorContext, ReaperTarget, TargetSection, TargetTypeDef,
    UnresolvedReaperTargetDef, VirtualPlaytimeSlot, DEFAULT_TARGET,
};
use playtime_api::persistence::SlotAddress;
use playtime_api::persistence::{ClipPlayStartTiming, ClipPlayStopTiming};
use realearn_api::persistence::PlaytimeSlotTransportAction;
use reaper_high::Project;

#[derive(Debug)]
pub struct UnresolvedPlaytimeSlotTransportTarget {
    pub slot: VirtualPlaytimeSlot,
    pub action: PlaytimeSlotTransportAction,
    pub options: ClipTransportOptions,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ClipTransportOptions {
    /// If this is on and one of the record actions is triggered, it will only have an effect if
    /// the record track of the clip column is armed.
    pub record_only_if_track_armed: bool,
    pub stop_column_if_slot_empty: bool,
    pub play_start_timing: Option<ClipPlayStartTiming>,
    pub play_stop_timing: Option<ClipPlayStopTiming>,
}

impl UnresolvedReaperTargetDef for UnresolvedPlaytimeSlotTransportTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: CompartmentKind,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context.project_or_current_project();
        let target = PlaytimeSlotTransportTarget {
            project,
            basics: ClipTransportTargetBasics {
                slot_coordinates: self.slot.resolve(context, compartment)?,
                action: self.action,
                options: self.options,
            },
        };
        Ok(vec![ReaperTarget::PlaytimeSlotTransportAction(target)])
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualPlaytimeSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaytimeSlotTransportTarget {
    pub project: Project,
    pub basics: ClipTransportTargetBasics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipTransportTargetBasics {
    pub slot_coordinates: SlotAddress,
    pub action: PlaytimeSlotTransportAction,
    pub options: ClipTransportOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeSlotTransportTarget {
    project: Project,
    basics: ClipTransportTargetBasics,
}

pub const PLAYTIME_SLOT_TRANSPORT_TARGET: TargetTypeDef = TargetTypeDef {
    lua_only: true,
    section: TargetSection::Playtime,
    name: "Slot transport action",
    short_name: "Playtime slot transport action",
    supports_track: false,
    supports_clip_slot: true,
    supports_real_time_control: true,
    ..DEFAULT_TARGET
};

#[cfg(not(feature = "playtime"))]
mod no_playtime_impl {
    use crate::domain::{
        ControlContext, PlaytimeSlotTransportTarget, RealTimeControlContext,
        RealTimeSlotTransportTarget, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeSlotTransportTarget {}
    impl<'a> Target<'a> for PlaytimeSlotTransportTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimeSlotTransportTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimeSlotTransportTarget {
        pub fn hit(
            &mut self,
            _value: ControlValue,
            _context: RealTimeControlContext,
        ) -> Result<bool, &'static str> {
            Err("Playtime not available")
        }
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use crate::domain::{
        format_value_as_on_off, transport_is_enabled_unit_value, Backbone,
        ClipTransportTargetBasics, CompoundChangeEvent, ControlContext, HitResponse,
        MappingControlContext, PlaytimeSlotTransportTarget, RealTimeControlContext,
        RealTimeReaperTarget, RealTimeSlotTransportTarget, RealearnTarget, ReaperTargetType,
        TargetCharacter,
    };
    use anyhow::bail;
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target, UnitValue};
    use playtime_api::persistence::SlotAddress;

    use crate::domain::playtime_util::{
        clip_play_state_unit_value, interpret_current_clip_slot_value,
    };
    use playtime_clip_engine::base::Column;
    #[cfg(feature = "playtime")]
    use playtime_clip_engine::{
        base::ClipMatrixEvent,
        rt::{
            ClipChangeEvent, ColumnPlaySlotOptions, InternalClipPlayState,
            QualifiedClipChangeEvent, QualifiedSlotChangeEvent, SlotChangeEvent,
        },
    };
    use realearn_api::persistence::PlaytimeSlotTransportAction;
    use reaper_high::{ChangeEvent, Project, Track};
    use std::borrow::Cow;

    impl PlaytimeSlotTransportTarget {
        pub fn clip_play_state(
            &self,
            matrix: &playtime_clip_engine::base::Matrix,
        ) -> Option<InternalClipPlayState> {
            let slot = matrix.find_slot(self.basics.slot_coordinates)?;
            if slot.is_empty() {
                return None;
            }
            Some(slot.play_state())
        }

        fn hit_internal(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> anyhow::Result<HitResponse> {
            use PlaytimeSlotTransportAction::*;
            Backbone::get().with_clip_matrix_mut(context.control_context.instance(), |matrix| {
                let response = match self.basics.action {
                    Trigger => {
                        let velocity = value.to_unit_value().map_err(anyhow::Error::msg)?;
                        matrix.trigger_slot(self.basics.slot_coordinates, velocity)?;
                        HitResponse::processed_with_effect()
                    }
                    PlayStop => {
                        if value.is_on() {
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
                        if value.is_on() {
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
                        if value.is_on() {
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
                        if value.is_on() {
                            matrix.pause_clip(self.basics.slot_coordinates)?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                    RecordStop => {
                        if value.is_on() {
                            if self.basics.options.record_only_if_track_armed
                                && !matrix.column_is_armed_for_recording(
                                    self.basics.slot_coordinates.column(),
                                )
                            {
                                bail!(NOT_RECORDING_BECAUSE_NOT_ARMED);
                            }
                            matrix.record_or_overdub_slot(self.basics.slot_coordinates)?;
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_coordinates,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    RecordPlayStop => {
                        if value.is_on() {
                            if matrix.slot_is_empty(self.basics.slot_coordinates) {
                                // Slot is empty.
                                if self.basics.options.record_only_if_track_armed {
                                    // Record only if armed.
                                    if matrix.column_is_armed_for_recording(
                                        self.basics.slot_coordinates.column(),
                                    ) {
                                        // Is armed, so record.
                                        matrix
                                            .record_or_overdub_slot(self.basics.slot_coordinates)?;
                                    } else if self.basics.options.stop_column_if_slot_empty {
                                        // Not armed but column stopping on empty slots enabled.
                                        // Since we already know that the slot is empty, we do
                                        // it explicitly without invoking play passing that option.
                                        matrix.stop_column(
                                            self.basics.slot_coordinates.column(),
                                            None,
                                        )?;
                                    } else {
                                        bail!(NOT_RECORDING_BECAUSE_NOT_ARMED);
                                    }
                                } else {
                                    // Definitely record.
                                    matrix.record_or_overdub_slot(self.basics.slot_coordinates)?;
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
                        if value.is_on() {
                            matrix.toggle_looped(self.basics.slot_coordinates)?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                };
                Ok(response)
            })?
        }
    }

    impl RealearnTarget for PlaytimeSlotTransportTarget {
        fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
            control_type_and_character(self.basics.action)
        }

        fn clip_slot_address(&self) -> Option<SlotAddress> {
            Some(self.basics.slot_coordinates)
        }

        fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
            format_value_as_on_off(value).to_string()
        }

        fn hit(
            &mut self,
            value: ControlValue,
            context: MappingControlContext,
        ) -> Result<HitResponse, &'static str> {
            self.hit_internal(value, context)
                .map_err(|_| "error while hitting clip transport target")
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
                    use PlaytimeSlotTransportAction::*;
                    match event {
                        SlotChangeEvent::PlayState(new_state) => match self.basics.action {
                            Trigger | PlayStop | PlayPause | Stop | Pause | RecordStop
                            | RecordPlayStop => {
                                tracing::debug!(
                                    "Changed play state of {} to {new_state:?}",
                                    self.basics.slot_coordinates
                                );
                                let uv = clip_play_state_unit_value(self.basics.action, *new_state);
                                (true, Some(AbsoluteValue::Continuous(uv)))
                            }
                            Looped => (false, None),
                        },
                        SlotChangeEvent::Clips(_) => (true, None),
                        _ => (false, None),
                    }
                }
                CompoundChangeEvent::Reaper(ChangeEvent::TrackArmChanged(_)) => (true, None),
                CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::ClipChanged(
                    QualifiedClipChangeEvent {
                        clip_address,
                        event,
                    },
                )) if clip_address.slot_address == self.basics.slot_coordinates => {
                    use PlaytimeSlotTransportAction::*;
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
            Some(ReaperTargetType::PlaytimeSlotTransportAction)
        }

        fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
            use PlaytimeSlotTransportAction::*;
            if matches!(self.basics.action, RecordStop | RecordPlayStop | Looped) {
                // These are not for real-time usage.
                return None;
            }
            let t = RealTimeSlotTransportTarget {
                project: self.project,
                basics: self.basics.clone(),
            };
            Some(RealTimeReaperTarget::ClipTransport(t))
        }

        fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
            match key {
                "slot_state.id" => Backbone::get()
                    .with_clip_matrix(context.instance(), |matrix| {
                        let id_string = match self.clip_play_state(matrix) {
                            None => {
                                // Slot is empty
                                match matrix.find_column(self.basics.slot_coordinates.column_index)
                                {
                                    None => "playtime.slot_state.empty",
                                    Some(col) => {
                                        if col.is_armed_for_recording() {
                                            "playtime.slot_state.armed"
                                        } else {
                                            "playtime.slot_state.empty"
                                        }
                                    }
                                }
                            }
                            Some(s) => s.id_string(),
                        };
                        Some(PropValue::Text(id_string.into()))
                    })
                    .ok()?,
                _ => None,
            }
        }
    }

    impl<'a> Target<'a> for PlaytimeSlotTransportTarget {
        type Context = ControlContext<'a>;

        fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
            let val = Backbone::get()
                .with_clip_matrix(context.instance(), |matrix| {
                    use PlaytimeSlotTransportAction::*;
                    let val = match self.basics.action {
                        Trigger | PlayStop | PlayPause | Stop | Pause | RecordStop
                        | RecordPlayStop => {
                            let play_state = self.clip_play_state(matrix)?;
                            tracing::trace!(
                                "Current play state of {}: {play_state:?}",
                                self.basics.slot_coordinates
                            );
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

    impl RealTimeSlotTransportTarget {
        /// This returns `Ok(false)` if real-time trigger was not possible **and** the consumer should
        /// try main-thread control instead (used for trigger action in order to possibly record when slot empty).
        pub fn hit(
            &mut self,
            value: ControlValue,
            context: RealTimeControlContext,
        ) -> Result<bool, &'static str> {
            use PlaytimeSlotTransportAction::*;
            let matrix = context.clip_matrix()?;
            let matrix = matrix.lock();
            match self.basics.action {
                Trigger => {
                    matrix.trigger_slot(self.basics.slot_coordinates, value.to_unit_value()?)
                }
                PlayStop => {
                    if value.is_on() {
                        matrix
                            .play_slot(self.basics.slot_coordinates, self.basics.play_options())?;
                    } else {
                        matrix.stop_slot(
                            self.basics.slot_coordinates,
                            self.basics.options.play_stop_timing,
                        )?;
                    }
                    Ok(true)
                }
                PlayPause => {
                    if value.is_on() {
                        matrix
                            .play_slot(self.basics.slot_coordinates, self.basics.play_options())?;
                    } else {
                        matrix.pause_slot(self.basics.slot_coordinates)?;
                    }
                    Ok(true)
                }
                Stop => {
                    if value.is_on() {
                        matrix.stop_slot(
                            self.basics.slot_coordinates,
                            self.basics.options.play_stop_timing,
                        )?;
                    }
                    Ok(true)
                }
                Pause => {
                    if value.is_on() {
                        matrix.pause_slot(self.basics.slot_coordinates)?;
                    }
                    Ok(true)
                }
                RecordStop | RecordPlayStop => Err("record not supported for real-time target"),
                Looped => Err("setting looped not supported for real-time target"),
            }
        }
    }

    impl<'a> Target<'a> for RealTimeSlotTransportTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            use PlaytimeSlotTransportAction::*;
            let matrix = context.clip_matrix().ok()?;
            let matrix = matrix.lock();
            let column = matrix.column(self.basics.slot_coordinates.column()).ok()?;
            let column = column.blocking_lock();
            let slot = column.slot(self.basics.slot_coordinates.row()).ok()?;
            let Some(first_clip) = slot.first_clip() else {
                return Some(AbsoluteValue::Continuous(UnitValue::MIN));
            };
            let val = match self.basics.action {
                Trigger | PlayStop | PlayPause | Stop | Pause | RecordStop | RecordPlayStop => {
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

    impl ClipTransportTargetBasics {
        fn play_options(&self) -> ColumnPlaySlotOptions {
            ColumnPlaySlotOptions {
                // TODO-high-ms5 Should we respect velocity for non-trigger transport actions as well?
                velocity: Some(UnitValue::MAX),
                stop_column_if_slot_empty: self.options.stop_column_if_slot_empty,
                start_timing: self.options.play_start_timing,
            }
        }
    }
    const NOT_RECORDING_BECAUSE_NOT_ARMED: &str = "not recording because not armed";

    fn control_type_and_character(
        action: PlaytimeSlotTransportAction,
    ) -> (ControlType, TargetCharacter) {
        use PlaytimeSlotTransportAction::*;
        match action {
            Trigger => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Continuous,
            ),
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
}
