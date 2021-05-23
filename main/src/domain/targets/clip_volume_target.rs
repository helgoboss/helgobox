use crate::domain::ui_util::{
    format_value_as_db, format_value_as_db_without_unit, parse_value_from_db,
    reaper_volume_unit_value,
};
use crate::domain::{
    ClipChangedEvent, ControlContext, InstanceFeedbackEvent, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::Volume;

#[derive(Clone, Debug, PartialEq)]
pub struct ClipVolumeTarget {
    pub slot_index: usize,
}

impl RealearnTarget for ClipVolumeTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
    }

    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_value_from_db(text)
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        format_value_as_db_without_unit(value)
    }

    fn value_unit(&self) -> &'static str {
        "dB"
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_db(value)
    }

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        let volume = Volume::try_from_soft_normalized_value(value.as_unit_value()?.get());
        let mut instance_state = context.instance_state.borrow_mut();
        instance_state.set_volume(
            self.slot_index,
            volume.unwrap_or(Volume::MIN).reaper_value(),
        )?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        // TODO-medium With clip targets we should check the control context (instance state) if
        //  slot filled.
        true
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            InstanceFeedbackEvent::ClipChanged {
                slot_index: si,
                event: ClipChangedEvent::ClipVolumeChanged(new_value),
            } if *si == self.slot_index => (true, Some(reaper_volume_unit_value(*new_value))),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for ClipVolumeTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext<'a>) -> Option<AbsoluteValue> {
        let instance_state = context.instance_state.borrow();
        let volume = instance_state.get_slot(self.slot_index).ok()?.volume();
        Some(AbsoluteValue::Continuous(reaper_volume_unit_value(volume)))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
