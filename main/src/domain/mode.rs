use crate::domain::EelTransformation;
use helgoboss_learn::{AbsoluteMode, ControlValue, RelativeMode, Target, ToggleMode, UnitValue};

// Represents a learn mode
#[derive(Clone, Debug)]
pub enum Mode {
    Absolute(AbsoluteMode<EelTransformation>),
    Relative(RelativeMode<EelTransformation>),
    Toggle(ToggleMode<EelTransformation>),
}

impl Mode {
    pub fn control(&mut self, value: ControlValue, target: &impl Target) -> Option<ControlValue> {
        use Mode::*;
        match self {
            Absolute(m) => m
                .control(value.as_absolute().ok()?, target)
                .map(ControlValue::Absolute),
            Relative(m) => m.control(value, target),
            Toggle(m) => m
                .control(value.as_absolute().ok()?, target)
                .map(ControlValue::Absolute),
        }
    }

    pub fn feedback(&self, value: UnitValue) -> Option<UnitValue> {
        use Mode::*;
        match self {
            Absolute(m) => m.feedback(value),
            Relative(m) => m.feedback(value),
            Toggle(m) => m.feedback(value),
        }
    }
}
