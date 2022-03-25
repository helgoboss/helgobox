use enumflags2::BitFlags;
use helgoboss_learn::{ControlValue, UnitValue};
use reaper_high::Reaper;
use reaper_medium::{Accel, AcceleratorBehavior, AcceleratorKey};
use std::fmt::{Display, Formatter};

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct KeySource {
    currently_pressed: bool,
    stroke: Keystroke,
}

impl KeySource {
    pub fn new(stroke: Keystroke) -> Self {
        Self {
            currently_pressed: false,
            stroke,
        }
    }

    pub fn stroke(&self) -> Keystroke {
        self.stroke
    }

    pub fn control(&mut self, value: KeyMessage) -> Option<ControlValue> {
        if value.pressed() && self.currently_pressed {
            // We don't want OS-triggered repeated key firing. We have our own fire modes :)
            return None;
        }
        let result = self.try_control(value)?;
        self.currently_pressed = value.pressed();
        Some(result)
    }

    pub fn try_control(&self, value: KeyMessage) -> Option<ControlValue> {
        if value.stroke != self.stroke {
            return None;
        }
        let value = if value.pressed() {
            UnitValue::MAX
        } else {
            UnitValue::MIN
        };
        Some(ControlValue::AbsoluteContinuous(value))
    }
}

impl Display for KeySource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.stroke.fmt(f)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct KeyMessage {
    pressed: bool,
    stroke: Keystroke,
}

impl KeyMessage {
    pub fn new(pressed: bool, stroke: Keystroke) -> Self {
        Self { pressed, stroke }
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }

    pub fn stroke(&self) -> Keystroke {
        self.stroke
    }
}

impl Display for KeyMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let kind_text = if self.pressed { "Pressed" } else { "Released" };
        write!(f, "{} {}", kind_text, self.stroke)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Keystroke {
    behavior: BitFlags<AcceleratorBehavior>,
    key: AcceleratorKey,
}

impl Keystroke {
    pub fn new(behavior: BitFlags<AcceleratorBehavior>, key: AcceleratorKey) -> Self {
        Self { behavior, key }
    }
}

impl Display for Keystroke {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let accel = Accel {
            f_virt: self.behavior,
            key: self.key,
            cmd: 0,
        };
        write!(
            f,
            "{}",
            Reaper::get().medium_reaper().kbd_format_key_name(accel)
        )
    }
}
