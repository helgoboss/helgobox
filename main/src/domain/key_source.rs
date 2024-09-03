use crate::domain::ControlOutcome;
use enumflags2::BitFlags;
use helgoboss_learn::{ControlValue, UnitValue};
use reaper_high::{AcceleratorKey, Reaper};
use reaper_medium::{
    virt_keys, Accel, AccelMsgKind, AcceleratorBehavior, AcceleratorKeyCode, ReaperString, VirtKey,
};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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
            // Oh yes, and there's "Char". If in a text field, Windows (and maybe also other OS?)
            // sends for each character key press an additional "Char" interaction. It should have
            // been normalized in the accelerator and match the keystroke of the key-down event.
            // As a result, we consume it as well.
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, derive_more::Display)]
pub enum KeyInteractionKind {
    Press,
    Release,
    Other,
}

impl KeyInteractionKind {
    pub fn is_press(&self) -> bool {
        matches!(self, Self::Press)
    }

    /// Checks if the kind is relevant (only key-down and key-up).
    pub fn is_press_or_release(&self) -> bool {
        matches!(self, Self::Press | Self::Release)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Keystroke {
    modifiers: BitFlags<AcceleratorBehavior>,
    key: AcceleratorKeyCode,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, derive_more::Display)]
pub enum KeyStrokePortability {
    NonPortable(PortabilityIssue),
    Portable,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, derive_more::Display)]
pub enum PortabilityIssue {
    NotNormalized,
    OperatingSystemRelated,
    KeyboardLayoutRelated,
    Other,
}

impl Keystroke {
    pub fn new(behavior: BitFlags<AcceleratorBehavior>, key: AcceleratorKeyCode) -> Self {
        Self {
            modifiers: behavior,
            key,
        }
    }

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
    ///   We don't like any. Mark them as non-portable!
    #[allow(clippy::if_same_then_else)]
    pub fn normalized(&self) -> Self {
        use AcceleratorBehavior::*;
        let mut modifiers = self.modifiers;
        let key = self.key;
        // Remove modifier info (makes a difference on macOS and Linux only).
        modifiers.remove(Shift | Control | Alt);
        // Do some Windows-specific conversions.
        #[cfg(windows)]
        {
            if modifiers.contains(VirtKey) {
                // Key is a virtual key.
                // On Windows, we need to convert virtual keys for umlauts or special characters to
                // character codes so we match the behavior of macOS and Linux.
                let character_code = unsafe {
                    winapi::um::winuser::MapVirtualKeyW(
                        key.get() as u32,
                        winapi::um::winuser::MAPVK_VK_TO_CHAR,
                    )
                };
                if character_code == 0 {
                    // Couldn't find corresponding character code.
                    Self::new(modifiers, key)
                } else if character_code == key.get() as u32 {
                    // Character code is equal to virtual key code. In this case, macOS and Linux
                    // would also use the virtual key code (I hope), so we keep it.
                    Self::new(modifiers, key)
                } else {
                    // We have a completely different character code. Use this one because
                    // macOS and Linux would also prefer the character code.
                    modifiers.remove(VirtKey);
                    Self::new(modifiers, AcceleratorKeyCode::new(character_code as u16))
                }
            } else {
                // Key is a character code. Use as is.
                Self::new(modifiers, key)
            }
        }
        // On Linux and macOS, this is not necessary.
        #[cfg(not(windows))]
        {
            Self::new(modifiers, key)
        }
    }

    pub fn modifiers(&self) -> BitFlags<AcceleratorBehavior> {
        self.modifiers
    }

    pub fn key_code(&self) -> AcceleratorKeyCode {
        self.key
    }

    /// Returns information about portability of this keystroke across operating systems, keyboards,
    /// layouts, if known.
    pub fn portability(&self) -> Option<KeyStrokePortability> {
        use KeyStrokePortability::*;
        use PortabilityIssue::*;
        let normalized = self.normalized();
        if *self != normalized {
            return Some(KeyStrokePortability::NonPortable(
                PortabilityIssue::NotNormalized,
            ));
        }
        match self.accelerator_key() {
            AcceleratorKey::Character(ch) => {
                match ch {
                    // Consider non-ASCII characters generally as non-portable.
                    x if x > 0x7f => Some(NonPortable(KeyboardLayoutRelated)),
                    a => {
                        let a = a as u8;
                        match a {
                            // These ones are at least on the numpad. Numpad is layout-agnostic.
                            b'+' | b'-' | b'*' | b'/' => Some(Portable),
                            // These have special behavior on some keyboard layouts.
                            b'`' | b'^' => Some(NonPortable(KeyboardLayoutRelated)),
                            // Since most ASCII characters are transmitted as virtual keys, we
                            // can categorize all other ASCII characters as probably not portable.
                            _ => None,
                        }
                    }
                }
            }
            AcceleratorKey::VirtKey(k) => {
                use virt_keys::*;
                match k {
                    // Special keys that either every keyboard has or everybody knows a keyboard
                    // might not have. Anyway, no cross-platform or keyboard-layout issues usually.
                    ESCAPE | F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | INSERT
                    | NUMPAD0 | NUMPAD1 | NUMPAD2 | NUMPAD3 | NUMPAD4 | NUMPAD5 | NUMPAD6
                    | NUMPAD7 | NUMPAD8 | NUMPAD9 | SHIFT | CONTROL | MENU | SPACE | TAB | HOME
                    | END | PRIOR | NEXT | LEFT | UP | DOWN | RIGHT | RETURN | BACK | PAUSE
                    | CLEAR | DELETE | SNAPSHOT => Some(Portable),
                    CAPITAL => {
                        // CAPS LOCK doesn't fire on macOS.
                        Some(NonPortable(OperatingSystemRelated))
                    }
                    F12 => {
                        // F12 is known to be treated a bit differently at times.
                        Some(NonPortable(PortabilityIssue::Other))
                    }
                    // Characters
                    k => match u8::try_from(k.get()) {
                        Ok(b'A'..=b'Z' | b'0'..=b'9') => Some(Portable),
                        // Other basic characters don't qualify as explicitly portable.
                        _ => None,
                    },
                }
            }
        }
    }

    pub fn accelerator_key(&self) -> AcceleratorKey {
        AcceleratorKey::from_behavior_and_key_code(self.modifiers, self.key)
    }

    pub fn is_modifier_key(&self) -> bool {
        use virt_keys::{CONTROL, MENU, SHIFT};
        match self.accelerator_key() {
            AcceleratorKey::VirtKey(k) if matches!(k, CONTROL | MENU | SHIFT) => true,
            _ => false,
        }
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
        let key = self.accelerator_key();
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
