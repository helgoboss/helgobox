use crate::domain::{
    format_value_as_on_off, fx_enable_unit_value, ControlContext, RealearnTarget, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct FxEnableTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxEnableTarget {
    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(&mut self, value: ControlValue, _: ControlContext) -> Result<(), &'static str> {
        if value.to_unit_value()?.is_zero() {
            self.fx.disable();
        } else {
            self.fx.enable();
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.fx.is_available()
    }

    fn project(&self) -> Option<Project> {
        self.fx.project()
    }

    fn track(&self) -> Option<&Track> {
        self.fx.track()
    }

    fn fx(&self) -> Option<&Fx> {
        Some(&self.fx)
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            ChangeEvent::FxEnabledChanged(e) if e.fx == self.fx => (
                true,
                Some(AbsoluteValue::Continuous(fx_enable_unit_value(e.new_value))),
            ),
            _ => (false, None),
        }
    }
}

impl<'a> Target<'a> for FxEnableTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::Continuous(fx_enable_unit_value(
            self.fx.is_enabled(),
        )))
    }

    fn control_type(&self) -> ControlType {
        self.control_type_and_character().0
    }
}
