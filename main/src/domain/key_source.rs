use crate::domain::ControlOutcome;
use enumflags2::BitFlags;
use helgoboss_learn::{ControlValue, UnitValue};
use reaper_high::Reaper;
use reaper_medium::{Accel, AccelMsgKind, AcceleratorBehavior, AcceleratorKey};
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

    pub fn control(&mut self, msg: KeyMessage) -> Option<ControlOutcome<ControlValue>> {
        if !msg.is_press_or_release() && msg.stroke() == self.stroke {
            // On Windows, there's not just press and release but also something like "key is being
            // hold", which fires continuously. We neither want to react to it (because we have our
            // own fire modes) nor simply forward it to REAPER (because it would dig a hole
            // into our "Filter matched events" mechanism). We let this source "consume" the message
            // instead.
            return Some(ControlOutcome::Consumed);
        }
        if msg.is_press() && self.currently_pressed {
            // We don't want OS-triggered repeated key firing (macOS). We have our own fire modes.
            return Some(ControlOutcome::Consumed);
        }
        let control_value = self.get_control_value(msg)?;
        self.currently_pressed = msg.is_press();
        Some(ControlOutcome::Matched(control_value))
    }

    /// Non-mutating! Used for checks.
    pub fn reacts_to_message_with(&self, msg: KeyMessage) -> Option<ControlValue> {
        if !msg.is_press_or_release() {
            return None;
        }
        self.get_control_value(msg)
    }

    /// Assumes that relevance has been checked already.
    fn get_control_value(&self, msg: KeyMessage) -> Option<ControlValue> {
        if msg.stroke != self.stroke {
            return None;
        }
        let value = if msg.is_press() {
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
    kind: AccelMsgKind,
    stroke: Keystroke,
}

impl KeyMessage {
    pub fn new(kind: AccelMsgKind, stroke: Keystroke) -> Self {
        Self { kind, stroke }
    }

    pub fn kind(&self) -> AccelMsgKind {
        self.kind
    }

    pub fn is_press(&self) -> bool {
        self.kind == AccelMsgKind::KeyDown
    }

    /// Checks if the kind is relevant (only key-down and key-up).
    pub fn is_press_or_release(&self) -> bool {
        matches!(self.kind, AccelMsgKind::KeyDown | AccelMsgKind::KeyUp)
    }

    pub fn stroke(&self) -> Keystroke {
        self.stroke
    }
}

impl Display for KeyMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let kind_text = match self.kind {
            AccelMsgKind::KeyDown => "Press",
            AccelMsgKind::KeyUp => "Release",
            _ => "Other",
        };
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
