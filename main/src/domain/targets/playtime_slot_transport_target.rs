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
    /// This is only ever taken care of from the main thread because only the main thread can decide whether
    /// to record *or* stop the column.
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
                slot_address: self.slot.resolve(context, compartment)?,
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
    pub slot_address: SlotAddress,
    pub action: PlaytimeSlotTransportAction,
    pub options: ClipTransportOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RealTimePlaytimeSlotTransportTarget {
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
        RealTimePlaytimeSlotTransportTarget, RealearnTarget,
    };
    use helgoboss_learn::{ControlValue, Target};

    impl RealearnTarget for PlaytimeSlotTransportTarget {}
    impl<'a> Target<'a> for PlaytimeSlotTransportTarget {
        type Context = ControlContext<'a>;
    }
    impl<'a> Target<'a> for RealTimePlaytimeSlotTransportTarget {
        type Context = RealTimeControlContext<'a>;
    }
    impl RealTimePlaytimeSlotTransportTarget {
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
        RealTimePlaytimeSlotTransportTarget, RealTimeReaperTarget, RealearnTarget,
        ReaperTargetType, TargetCharacter,
    };
    use anyhow::bail;
    use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, PropValue, Target, UnitValue};
    use playtime_api::persistence::SlotAddress;

    use crate::domain::playtime_util::{
        clip_play_state_unit_value, interpret_current_clip_slot_value,
    };
    use playtime_clip_engine::rt::TriggerSlotMainOptions;
    #[cfg(feature = "playtime")]
    use playtime_clip_engine::{
        base::ClipMatrixEvent,
        rt::{
            ClipChangeEvent, ColumnPlaySlotOptions, InternalClipPlayState,
            QualifiedClipChangeEvent, QualifiedSlotChangeEvent, SlotChangeEvent,
        },
    };
    use realearn_api::persistence::PlaytimeSlotTransportAction;
    use reaper_high::{ChangeEvent, Project};
    use std::borrow::Cow;

    impl PlaytimeSlotTransportTarget {
        pub fn clip_play_state(
            &self,
            matrix: &playtime_clip_engine::base::Matrix,
        ) -> Option<InternalClipPlayState> {
            let slot = matrix.find_slot(self.basics.slot_address)?;
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
                        // This action is special in that it some of it can be carried out directly in the real-time
                        // thread **but not everything**! Other parts can only be done from main thread, such as
                        // starting to record into an empty slot and/or auto-activating the triggered slot. The
                        // stuff done in the main thread (play/stop) must NOT be repeated here.
                        let velocity = value.to_unit_value().map_err(anyhow::Error::msg)?;
                        matrix.trigger_slot(
                            self.basics.slot_address,
                            velocity,
                            TriggerSlotMainOptions {
                                stop_column_if_slot_empty: self
                                    .basics
                                    .options
                                    .stop_column_if_slot_empty,
                                allow_activate: true,
                                // If control was initiated from real-time target, don't take care of starting/stopping.
                                allow_start_stop: !context.coming_from_real_time,
                            },
                        )?;
                        HitResponse::processed_with_effect()
                    }
                    PlayStop => {
                        if value.is_on() {
                            matrix
                                .play_slot(self.basics.slot_address, self.basics.play_options())?;
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_address,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    PlayPause => {
                        if value.is_on() {
                            matrix
                                .play_slot(self.basics.slot_address, self.basics.play_options())?;
                        } else {
                            matrix.pause_clip(self.basics.slot_address)?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    Stop => {
                        if value.is_on() {
                            matrix.stop_slot(
                                self.basics.slot_address,
                                self.basics.options.play_stop_timing,
                            )?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                    Pause => {
                        if value.is_on() {
                            matrix.pause_clip(self.basics.slot_address)?;
                            HitResponse::processed_with_effect()
                        } else {
                            HitResponse::ignored()
                        }
                    }
                    RecordStop => {
                        if value.is_on() {
                            if self.basics.options.record_only_if_track_armed
                                && !matrix.column_is_armed_for_recording(
                                    self.basics.slot_address.column(),
                                )
                            {
                                bail!(NOT_RECORDING_BECAUSE_NOT_ARMED);
                            }
                            matrix.record_or_overdub_slot(self.basics.slot_address)?;
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_address,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    RecordPlayStop => {
                        if value.is_on() {
                            if matrix.slot_is_empty(self.basics.slot_address) {
                                // Slot is empty.
                                if self.basics.options.record_only_if_track_armed {
                                    // Record only if armed.
                                    if matrix.column_is_armed_for_recording(
                                        self.basics.slot_address.column(),
                                    ) {
                                        // Is armed, so record.
                                        matrix.record_or_overdub_slot(self.basics.slot_address)?;
                                    } else if self.basics.options.stop_column_if_slot_empty {
                                        // Not armed but column stopping on empty slots enabled.
                                        // Since we already know that the slot is empty, we do
                                        // it explicitly without invoking play passing that option.
                                        matrix
                                            .stop_column(self.basics.slot_address.column(), None)?;
                                    } else {
                                        bail!(NOT_RECORDING_BECAUSE_NOT_ARMED);
                                    }
                                } else {
                                    // Definitely record.
                                    matrix.record_or_overdub_slot(self.basics.slot_address)?;
                                }
                            } else {
                                // Slot is filled.
                                matrix.play_slot(
                                    self.basics.slot_address,
                                    self.basics.play_options(),
                                )?;
                            }
                        } else {
                            matrix.stop_slot(
                                self.basics.slot_address,
                                self.basics.options.play_stop_timing,
                            )?;
                        }
                        HitResponse::processed_with_effect()
                    }
                    Looped => {
                        if value.is_on() {
                            matrix.toggle_looped(self.basics.slot_address)?;
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
            Some(self.basics.slot_address)
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
                )) if *sc == self.basics.slot_address => {
                    use PlaytimeSlotTransportAction::*;
                    match event {
                        SlotChangeEvent::PlayState(new_state) => match self.basics.action {
                            Trigger | PlayStop | PlayPause | Stop | Pause | RecordStop
                            | RecordPlayStop => {
                                tracing::debug!(
                                    "Changed play state of {} to {new_state:?}",
                                    self.basics.slot_address
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
                )) if clip_address.slot_address == self.basics.slot_address => {
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
            let t = RealTimePlaytimeSlotTransportTarget {
                project: self.project,
                basics: self.basics.clone(),
            };
            Some(RealTimeReaperTarget::PlaytimeSlotTransport(t))
        }

        fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
            match key {
                "slot_state.id" => Backbone::get()
                    .with_clip_matrix(context.instance(), |matrix| {
                        let id_string = match self.clip_play_state(matrix) {
                            None => {
                                // Slot is empty
                                match matrix.find_column(self.basics.slot_address.column_index) {
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
                                self.basics.slot_address
                            );
                            clip_play_state_unit_value(self.basics.action, play_state)
                        }
                        Looped => {
                            let is_looped =
                                matrix.find_slot(self.basics.slot_address)?.looped().ok()?;
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

    impl RealTimePlaytimeSlotTransportTarget {
        /// This returns `Ok(true)` if the event should still be forwarded to the main thread because there's still
        /// something to do but it can only be done in the main thread (e.g. when triggering in order to possibly
        /// record when slot empty or to activate the slot).
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
                    // Whenever the value is on, we want to also forward control to the main (non-real-time) target.
                    // Reasons:
                    // 1. Only the main target can take care of activating the slot (`activate_slot_on_trigger`).
                    // 2. Only the main target can check whether the track is armed and start recording.
                    let forward_to_main_thread = value.is_on();
                    let execute_in_real_time = if forward_to_main_thread {
                        // We must carefully separate between what we do *here* (in real-time) and in the main target.
                        if matrix.slot_is_filled(self.basics.slot_address) {
                            // Slot is filled, so all the main thread will do is to activate the slot.
                            // We can safely execute.
                            true
                        } else {
                            // Careful! In addition to slot activation, the main target could decide to record.
                            // In that case we don't want to do anything in real-time. If `stop_column_if_slot_empty`
                            // is enabled, it's the responsibility to do that if it decides to *not record*.
                            false
                        }
                    } else {
                        // Easy. No main target that can get interfere.
                        true
                    };
                    if execute_in_real_time {
                        matrix.trigger_slot(self.basics.slot_address, value.to_unit_value()?)?;
                    }
                    Ok(forward_to_main_thread)
                }
                PlayStop => {
                    if value.is_on() {
                        matrix.play_slot(self.basics.slot_address, self.basics.play_options())?;
                    } else {
                        matrix.stop_slot(
                            self.basics.slot_address,
                            self.basics.options.play_stop_timing,
                        )?;
                    }
                    Ok(false)
                }
                PlayPause => {
                    if value.is_on() {
                        matrix.play_slot(self.basics.slot_address, self.basics.play_options())?;
                    } else {
                        matrix.pause_slot(self.basics.slot_address)?;
                    }
                    // Completely handled in real-time, no need to forward to main thread
                    Ok(false)
                }
                Stop => {
                    if value.is_on() {
                        matrix.stop_slot(
                            self.basics.slot_address,
                            self.basics.options.play_stop_timing,
                        )?;
                    }
                    // Completely handled in real-time, no need to forward to main thread
                    Ok(false)
                }
                Pause => {
                    if value.is_on() {
                        matrix.pause_slot(self.basics.slot_address)?;
                    }
                    // Completely handled in real-time, no need to forward to main thread
                    Ok(false)
                }
                RecordStop | RecordPlayStop => Err("record not supported for real-time target"),
                Looped => Err("setting looped not supported for real-time target"),
            }
        }
    }

    impl<'a> Target<'a> for RealTimePlaytimeSlotTransportTarget {
        type Context = RealTimeControlContext<'a>;

        fn current_value(&self, context: RealTimeControlContext<'a>) -> Option<AbsoluteValue> {
            use PlaytimeSlotTransportAction::*;
            let matrix = context.clip_matrix().ok()?;
            let matrix = matrix.lock();
            let column = matrix.column(self.basics.slot_address.column()).ok()?;
            let column = column.blocking_lock();
            let slot = column.slot(self.basics.slot_address.row()).ok()?;
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
                // TODO-high-playtime-before-release Should we respect velocity for non-trigger transport actions as well?
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
