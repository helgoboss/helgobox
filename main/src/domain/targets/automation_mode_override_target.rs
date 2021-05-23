use crate::domain::{
    format_value_as_on_off, global_automation_mode_override_unit_value, ControlContext,
    RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::GlobalAutomationModeOverride;

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationModeOverrideTarget {
    pub mode_override: Option<GlobalAutomationModeOverride>,
}

impl RealearnTarget for AutomationModeOverrideTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        // Retriggerable because of #277
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn control(&self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.as_unit_value()?.is_zero() {
            Reaper::get().set_global_automation_override(None);
        } else {
            Reaper::get().set_global_automation_override(self.mode_override);
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        match evt {
            ChangeEvent::GlobalAutomationOverrideChanged(e) => (
                true,
                Some(global_automation_mode_override_unit_value(
                    self.mode_override,
                    e.new_value,
                )),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for AutomationModeOverrideTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        let value = global_automation_mode_override_unit_value(
            self.mode_override,
            Reaper::get().global_automation_override(),
        );
        Some(AbsoluteValue::Continuous(value))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
