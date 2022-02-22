use reaper_medium::PositionInSeconds;

use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};

use crate::domain::{
    AdditionalFeedbackEvent, CompoundChangeEvent, ControlContext, ExtendedProcessorContext,
    FeedbackResolution, HitInstructionReturnValue, InstanceStateChanged, MappingCompartment,
    MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use playtime_clip_engine::processing::{ClipChangedEvent, ClipPlayState};
use playtime_clip_engine::{clip_timeline, Timeline};

#[derive(Debug)]
pub struct UnresolvedClipSeekTarget {
    pub slot_index: usize,
    pub feedback_resolution: FeedbackResolution,
}

impl UnresolvedReaperTargetDef for UnresolvedClipSeekTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::ClipSeek(ClipSeekTarget {
            slot_index: self.slot_index,
            feedback_resolution: self.feedback_resolution,
        })])
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipSeekTarget {
    pub slot_index: usize,
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
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let value = value.to_unit_value()?;
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        instance_state
            .clip_matrix_mut()
            .seek_clip_legacy(self.slot_index, value)?;
        Ok(None)
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
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::BeatChanged(_))
                if self.feedback_resolution == FeedbackResolution::Beat =>
            {
                (true, None)
            }
            CompoundChangeEvent::Instance(InstanceStateChanged::Clip {
                slot_index: si,
                event,
            }) if *si == self.slot_index => match event {
                // If feedback resolution is high, we use the special ClipChangedEvent to do our job
                // (in order to not lock mutex of playing clips more than once per main loop cycle).
                ClipChangedEvent::ClipPosition(new_position)
                    if self.feedback_resolution == FeedbackResolution::High =>
                {
                    (true, Some(AbsoluteValue::Continuous(*new_position)))
                }
                ClipChangedEvent::PlayState(ClipPlayState::Stopped) => {
                    (true, Some(AbsoluteValue::Continuous(UnitValue::MIN)))
                }
                _ => (false, None),
            },
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        let seconds = self.position_in_seconds(context)?;
        Some(format!("{:.3} s", seconds.get()))
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
        let instance_state = context.instance_state.borrow();
        let timeline = clip_timeline(context.processor_context.project(), false);
        let timeline_tempo = timeline.tempo_at(timeline.cursor_pos());
        instance_state
            .clip_matrix()
            .clip_position_in_seconds(self.slot_index, timeline_tempo)
    }
}

impl<'a> Target<'a> for ClipSeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let val = instance_state
            .clip_matrix()
            .proportional_clip_position_legacy(self.slot_index)?;
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const CLIP_SEEK_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Clip: Seek",
    short_name: "Clip seek",
    supports_feedback_resolution: true,
    supports_slot: true,
    ..DEFAULT_TARGET
};
