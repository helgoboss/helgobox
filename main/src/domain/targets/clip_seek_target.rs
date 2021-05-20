use crate::domain::{
    AdditionalFeedbackEvent, ClipChangedEvent, ClipPlayState, ControlContext, FeedbackResolution,
    InstanceFeedbackEvent, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{ControlType, ControlValue, Target, UnitValue};

#[derive(Clone, Debug, PartialEq)]
pub struct ClipSeekTarget {
    pub slot_index: usize,
    pub feedback_resolution: FeedbackResolution,
}

impl RealearnTarget for ClipSeekTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        let value = value.as_unit_value()?;
        let mut instance_state = context.instance_state.borrow_mut();
        instance_state.seek_slot(self.slot_index, value)?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
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
    ) -> (bool, Option<UnitValue>) {
        // When feedback resolution is beat, we only react to the main timeline beat changes.
        if self.feedback_resolution != FeedbackResolution::High {
            return (false, None);
        }
        match evt {
            InstanceFeedbackEvent::ClipChanged {
                slot_index: si,
                event,
            } if *si == self.slot_index => match event {
                ClipChangedEvent::ClipPositionChanged(new_position) => (true, Some(*new_position)),
                ClipChangedEvent::PlayStateChanged(ClipPlayState::Stopped) => {
                    (true, Some(UnitValue::MIN))
                }
                _ => (false, None),
            },
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for ClipSeekTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<UnitValue> {
        let instance_state = context.instance_state.borrow();
        let val = instance_state
            .get_slot(self.slot_index)
            .ok()?
            .position()
            .ok()?;
        Some(val)
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
