use crate::domain::ControlOutcome;
use enumflags2::BitFlags;
use helgoboss_learn::{ControlValue, UnitValue};
use reaper_high::{AcceleratorKey, Reaper};
use reaper_medium::{
    virt_keys, Accel, AccelMsgKind, AcceleratorBehavior, AcceleratorKeyCode, ReaperString, VirtKey,
};
use std::borrow::Cow;
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
        if !msg.interaction_kind().is_press_or_release() && msg.stroke() == self.stroke {
            // On Windows, there's not just press and release but also something like "key is being
            // hold", which fires continuously. We neither want to react to it (because we have our
            // own fire modes) nor simply forward it to REAPER (because it would dig a hole
            // into our "Filter matched events" mechanism). We let this source "consume" the message
            // instead.
            return Some(ControlOutcome::Consumed);
        }
        let is_press = msg.interaction_kind().is_press();
        if is_press && self.currently_pressed {
            // We don't want OS-triggered repeated key firing (macOS). We have our own fire modes.
            return Some(ControlOutcome::Consumed);
        }
        let control_value = self.get_control_value(msg)?;
        self.currently_pressed = is_press;
        Some(ControlOutcome::Matched(control_value))
    }

    /// Non-mutating! Used for checks.
    pub fn reacts_to_message_with(&self, msg: KeyMessage) -> Option<ControlValue> {
        if !msg.interaction_kind().is_press_or_release() {
            return None;
        }
        self.get_control_value(msg)
    }

    /// Assumes that relevance has been checked already.
    fn get_control_value(&self, msg: KeyMessage) -> Option<ControlValue> {
        if msg.stroke != self.stroke {
            return None;
        }
        let value = if msg.interaction_kind().is_press() {
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

    pub fn interaction_kind(&self) -> KeyInteractionKind {
        use AccelMsgKind::*;
        match self.kind {
            KeyDown | SysKeyDown => KeyInteractionKind::Press,
            KeyUp | SysKeyUp => KeyInteractionKind::Release,
            _ => KeyInteractionKind::Other,
        }
    }

    pub fn stroke(&self) -> Keystroke {
        self.stroke
    }
}

impl Display for KeyMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.interaction_kind(), self.stroke)
    }
}

#[derive(Copy, Clone, PartialEq, Debug, derive_more::Display)]
pub enum KeyInteractionKind {
    Press,
    Release,
    Other,
}

impl KeyInteractionKind {
    pub fn is_press(&self) -> bool {
        matches!(self, Self::Press)
    }

    pub fn is_release(&self) -> bool {
        matches!(self, Self::Release)
    }

    /// Checks if the kind is relevant (only key-down and key-up).
    pub fn is_press_or_release(&self) -> bool {
        matches!(self, Self::Press | Self::Release)
    }
}

#[derive(Copy, Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Keystroke {
    modifiers: BitFlags<AcceleratorBehavior>,
    key: AcceleratorKeyCode,
}

impl Keystroke {
    /// This normalizes the given behavior/key combination so it works cross-platform.
    ///
    /// When REAPER notifies us about incoming key events, the accelerator behavior and key codes
    /// look slightly different depending on the operating system:
    ///
    /// - On all operating systems, if we have a key combination, we receive each key event
    ///   separately, even the modifier keys. Good!
    /// - If we have a key combination (modifier key + normal key), Windows doesn't mention the
    ///   modifier keys in the accelerator behavior, but macOS and Linux do. We prefer the Windows
    ///   way because it makes more sense in this context. We receive modifier key-ups and key-downs
    ///   separately anyway.
    /// - On Windows, umlauts are delivered as virtual keys, on macOS and Linux as character codes.
    ///   We prefer the macOS and Linux way.
    /// - On Windows, "normal" special characters such as # and + are delivered as virtual keys.
    ///   On macOS and Linux, they are delivered as character codes.
    /// - On Windows, "abnormal" special characters such as ^ or ` are delivered as virtual keys.
    ///   On macOS, they are also delivered as virtual keys but with a different code.
    ///   On Linux, they are delivered as character code.
    ///   We don't like any. Mark them as not portable!
    pub fn normalized(
        mut behavior: BitFlags<AcceleratorBehavior>,
        key: AcceleratorKeyCode,
    ) -> Self {
        // Remove modifier info (makes a difference on macOS and Linux only).
        use AcceleratorBehavior::*;
        behavior.remove(Shift | Control | Alt);
        #[cfg(windows)]
        {
            // On Windows, we need to convert virtual keys for umlauts or special characters to
            // character codes so we match the behavior of macOS and Linux.
            if behavior.contains(VirtKey) {
                let character_code = unsafe {
                    winapi::um::winuser::MapVirtualKeyW(
                        key.get() as u32,
                        winapi::um::winuser::MAPVK_VK_TO_CHAR,
                    )
                };
                if character_code == 0 {
                    // Couldn't find corresponding character code.
                    Self::new(behavior, key)
                } else if character_code == key.get() as u32 {
                    // Character code is equal to virtual key code. In this case, macOS and Linux
                    // would also use the virtual key code (I hope), so we keep it.
                    Self::new(behavior, key)
                } else {
                    // We have a completely different character code. Use this one because
                    // macOS and Linux would also prefer the character code.
                    behavior.remove(VirtKey);
                    Self::new(behavior, AcceleratorKeyCode::new(character_code as u16))
                }
            } else {
                Self::new(behavior, key)
            }
        }
        #[cfg(not(windows))]
        {
            Self::new(behavior, key)
        }
    }

    pub fn new(behavior: BitFlags<AcceleratorBehavior>, key: AcceleratorKeyCode) -> Self {
        Self {
            modifiers: behavior,
            key,
        }
    }

    pub fn modifiers(&self) -> BitFlags<AcceleratorBehavior> {
        self.modifiers
    }

    pub fn key(&self) -> AcceleratorKeyCode {
        self.key
    }

    fn format_key_via_reaper(&self) -> ReaperString {
        let accel = Accel {
            f_virt: self.modifiers,
            key: self.key,
            cmd: 0,
        };
        Reaper::get().medium_reaper().kbd_format_key_name(accel)
    }
}

impl Display for Keystroke {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let key = AcceleratorKey::from_behavior_and_key_code(self.modifiers, self.key);
        use virt_keys::{CONTROL, MENU, SHIFT};
        const WIN: VirtKey = VirtKey::new(91);
        use AcceleratorKey as K;
        let label: Cow<str> = match key {
            K::VirtKey(SHIFT) => "Shift".into(),
            K::VirtKey(CONTROL) => "Ctrl/Cmd".into(),
            K::VirtKey(MENU) => "Alt/Opt".into(),
            K::VirtKey(WIN) => "Win/^".into(),
            _ => self.format_key_via_reaper().into_string().into(),
        };
        f.write_str(label.as_ref())
    }
}
