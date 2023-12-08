use reaper_medium::PositionInSeconds;
use std::borrow::Cow;

use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};

use crate::domain::{
    interpret_current_clip_slot_value, AdditionalFeedbackEvent, BackboneState, Compartment,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, FeedbackResolution, HitResponse,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, VirtualClipSlot, DEFAULT_TARGET,
};
use playtime_clip_engine::base::{ClipMatrixEvent, ClipSlotAddress};
use playtime_clip_engine::rt::supplier::audio::GlobalBlockProvider;
use playtime_clip_engine::rt::{
    ClipPlayState, InternalClipPlayState, QualifiedSlotChangeEvent, SlotChangeEvent,
};

#[derive(Debug)]
pub struct UnresolvedClipSeekTarget {
    pub slot: VirtualClipSlot,
    pub feedback_resolution: FeedbackResolution,
}

impl UnresolvedReaperTargetDef for UnresolvedClipSeekTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let target = ClipSeekTarget {
            slot_coordinates: self.slot.resolve(context, compartment)?,
            feedback_resolution: self.feedback_resolution,
        };
        Ok(vec![ReaperTarget::ClipSeek(target)])
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        // We always report beat resolution, even if feedback resolution is "high", in order to NOT
        // be continuously queried on each main loop iteration as part of ReaLearn's generic main
        // loop polling. There's a special clip polling logic in the main processor which detects
        // if there were any position changes. If there were, process_change_event() will be
        // called on the clip seek target. We need to do that special clip polling in any case.
        // If we would doing it a second time in ReaLearn's generic main loop polling, that would be
        // wasteful, in particular we would have to lock the clip preview register mutex twice
        // per main loop.
        Some(FeedbackResolution::Beat)
    }

    fn clip_slot_descriptor(&self) -> Option<&VirtualClipSlot> {
        Some(&self.slot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipSeekTarget {
    pub slot_coordinates: ClipSlotAddress,
    pub feedback_resolution: FeedbackResolution,
}

impl RealearnTarget for ClipSeekTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let value = value.to_unit_value()?;
        BackboneState::get()
            .with_clip_matrix(
                context.control_context.instance_state,
                |matrix| -> anyhow::Result<HitResponse> {
                    matrix.seek_slot(self.slot_coordinates, value)?;
                    Ok(HitResponse::processed_with_effect())
                },
            )
            .map_err(|_| "couldn't acquire matrix")?
            .map_err(|_| "couldn't carry out seek action")
    }

    fn is_available(&self, _: ControlContext) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }
    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            // When feedback resolution is beat, we only react to the main timeline beat changes.
            // TODO-low Beat-changed events are emitted only when the project is playing (important)
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::BeatChanged(_))
                if self.feedback_resolution == FeedbackResolution::Beat =>
            {
                (true, None)
            }
            CompoundChangeEvent::ClipMatrix(ClipMatrixEvent::SlotChanged(
                QualifiedSlotChangeEvent {
                    slot_address: si,
                    event,
                },
            )) if *si == self.slot_coordinates => match event {
                // If feedback resolution is high, we use the special ClipChangedEvent to do our job
                // (in order to not lock mutex of playing clips more than once per main loop cycle).
                SlotChangeEvent::Continuous(events)
                    if self.feedback_resolution == FeedbackResolution::High =>
                {
                    if let Some(first_event) = events.first() {
                        (
                            true,
                            Some(AbsoluteValue::Continuous(first_event.proportional)),
                        )
                    } else {
                        (false, None)
                    }
                }
                SlotChangeEvent::PlayState(InternalClipPlayState(
                    ClipPlayState::Stopped | ClipPlayState::Ignited,
                )) => (true, Some(AbsoluteValue::Continuous(UnitValue::MIN))),
                _ => (false, None),
            },
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let seconds = self.position_in_seconds(context)?;
        Some(format!("{:.3} s", seconds.get()).into())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let seconds = self.position_in_seconds(context)?;
        Some(NumericValue::Decimal(seconds.get()))
    }

    fn numeric_value_unit(&self, _: ControlContext) -> &'static str {
        "s"
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::ClipSeek)
    }
}

impl ClipSeekTarget {
    fn position_in_seconds(&self, context: ControlContext) -> Option<PositionInSeconds> {
        BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                let slot = matrix.find_slot(self.slot_coordinates)?;
                let timeline = matrix.timeline();
                let tempo = timeline.next_block().tempo_entry.props.tempo;
                slot.relevant_contents()
                    .primary_position_in_seconds(tempo)
                    .ok()
            })
            .ok()?
    }
}

impl<'a> Target<'a> for ClipSeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let val = BackboneState::get()
            .with_clip_matrix(context.instance_state, |matrix| {
                let relevant_content = matrix.find_slot(self.slot_coordinates)?.relevant_contents();
                let val = relevant_content
                    .primary_logical_proportional_position()
                    .ok()?;
                Some(AbsoluteValue::Continuous(val))
            })
            .ok()?;
        interpret_current_clip_slot_value(val)
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_SEEK_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Seek",
    short_name: "Clip seek",
    supports_feedback_resolution: true,
    supports_clip_slot: true,
    ..DEFAULT_TARGET
};
