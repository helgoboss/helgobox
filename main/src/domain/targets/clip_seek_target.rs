use crate::domain::{
    AdditionalFeedbackEvent, ClipChangedEvent, ClipPlayState, ControlContext, FeedbackResolution,
    HitInstructionReturnValue, InstanceFeedbackEvent, MappingControlContext, RealearnTarget,
    TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};

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
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<AbsoluteValue>) {
        // When feedback resolution is beat, we only react to the main timeline beat changes.
        if self.feedback_resolution != FeedbackResolution::High {
            return (false, None);
        }
        match evt {
            InstanceFeedbackEvent::ClipChanged {
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
