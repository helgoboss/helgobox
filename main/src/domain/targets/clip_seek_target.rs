use crate::domain::{
    AdditionalFeedbackEvent, ClipChangedEvent, ClipPlayState, CompoundChangeEvent, ControlContext,
    FeedbackResolution, HitInstructionReturnValue, InstanceStateChanged, MappingControlContext,
    RealearnTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, DEFAULT_TARGET_TYPE_DEF,
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
            // If feedback resolution is high, we use the special ClipChangedEvent to do our job
            // (in order to not lock mutex of playing clips more than once per main loop cycle).
            CompoundChangeEvent::Instance(InstanceStateChanged::Clip {
                slot_index: si,
                event,
            }) if self.feedback_resolution == FeedbackResolution::High
                && *si == self.slot_index =>
            {
                match event {
                    ClipChangedEvent::ClipPosition(new_position) => {
                        (true, Some(AbsoluteValue::Continuous(*new_position)))
                    }
                    ClipChangedEvent::PlayState(ClipPlayState::Stopped) => {
                        (true, Some(AbsoluteValue::Continuous(UnitValue::MIN)))
                    }
                    _ => (false, None),
                }
            }
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

pub const CLIP_SEEK_TARGET_TYPE_DEF: TargetTypeDef = TargetTypeDef {
    short_name: "Clip seek",
    supports_feedback_resolution: true,
    supports_slot: true,
    ..DEFAULT_TARGET_TYPE_DEF
};
