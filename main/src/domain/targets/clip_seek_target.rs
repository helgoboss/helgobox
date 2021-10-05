use crate::domain::{
    AdditionalFeedbackEvent, ClipChangedEvent, ClipPlayState, ControlContext, FeedbackResolution,
    HitInstructionReturnValue, InstanceStateChanged, MappingControlContext, RealearnTarget,
    ReaperTargetType, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, NumericValue, Target, UnitValue};
use reaper_medium::PositionInSeconds;

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
        instance_state.seek_slot(self.slot_index, value)?;
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        // If feedback resolution is high, we use the special ClipChangedEvent to do our job
        // (in order to not lock mutex of playing clips more than once per main loop cycle).
        if self.feedback_resolution == FeedbackResolution::Beat
            && matches!(evt, AdditionalFeedbackEvent::BeatChanged(_))
        {
            return (true, None);
        }
        (false, None)
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceStateChanged,
    ) -> (bool, Option<AbsoluteValue>) {
        // When feedback resolution is beat, we only react to the main timeline beat changes.
        if self.feedback_resolution != FeedbackResolution::High {
            return (false, None);
        }
        match evt {
            InstanceStateChanged::Clip {
                slot_index: si,
                event,
            } if *si == self.slot_index => match event {
                ClipChangedEvent::ClipPosition(new_position) => {
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
        let secs = instance_state
            .get_slot(self.slot_index)
            .ok()?
            .position_in_seconds();
        Some(secs)
    }
}

impl<'a> Target<'a> for ClipSeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let val = instance_state
            .get_slot(self.slot_index)
            .ok()?
            .position()
            .ok()?;
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}
