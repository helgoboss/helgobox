use crate::domain::{
    format_value_as_on_off, fx_enable_unit_value, ControlContext, HitInstructionReturnValue,
    MappingControlContext, RealearnTarget, ReaperTargetType, TargetCharacter,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{ChangeEvent, Fx, Project, Track};

#[derive(Clone, Debug, PartialEq)]
pub struct FxEnableTarget {
    pub fx: Fx,
}

impl RealearnTarget for FxEnableTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if value.to_unit_value()?.is_zero() {
            self.fx.disable();
        } else {
            self.fx.enable();
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
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

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::FxEnable)
    }
}

impl<'a> Target<'a> for FxEnableTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        Some(AbsoluteValue::Continuous(fx_enable_unit_value(
            self.fx.is_enabled(),
        )))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}
