use crate::domain::ui_util::{format_as_percentage_without_unit, parse_unit_value_from_percentage};
use crate::domain::{ExtendedSourceCharacter, TargetCharacter};
use ascii::{AsciiStr, AsciiString, ToAsciiChar};
use helgoboss_learn::{ControlType, ControlValue, SourceCharacter, Target, UnitValue};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::fmt::Display;
use std::iter::FromIterator;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VirtualTarget {
    control_element: VirtualControlElement,
}

impl VirtualTarget {
    pub fn new(control_element: VirtualControlElement) -> VirtualTarget {
        VirtualTarget { control_element }
    }

    pub fn control_element(&self) -> VirtualControlElement {
        self.control_element
    }

    pub fn character(&self) -> TargetCharacter {
        use VirtualControlElement::*;
        match self.control_element {
            Multi(_) => TargetCharacter::VirtualMulti,
            Button(_) => TargetCharacter::VirtualButton,
        }
    }
}

impl Target for VirtualTarget {
    fn current_value(&self) -> Option<UnitValue> {
        None
    }

    fn control_type(&self) -> ControlType {
        use VirtualControlElement::*;
        match self.control_element {
            Multi(_) => ControlType::VirtualMulti,
            Button(_) => ControlType::VirtualButton,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VirtualSource {
    control_element: VirtualControlElement,
}

impl VirtualSource {
    pub fn new(control_element: VirtualControlElement) -> VirtualSource {
        VirtualSource { control_element }
    }

    pub fn from_source_value(source_value: VirtualSourceValue) -> VirtualSource {
        VirtualSource::new(source_value.control_element)
    }

    pub fn control_element(&self) -> VirtualControlElement {
        self.control_element
    }

    pub fn control(&self, value: &VirtualSourceValue) -> Option<ControlValue> {
        if self.control_element != value.control_element {
            return None;
        }
        Some(value.control_value)
    }

    pub fn feedback(&self, feedback_value: UnitValue) -> VirtualSourceValue {
        VirtualSourceValue::new(self.control_element, ControlValue::Absolute(feedback_value))
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        let absolute_value = value.as_absolute()?;
        Ok(format_as_percentage_without_unit(absolute_value))
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_unit_value_from_percentage(text)
    }

    pub fn character(&self) -> ExtendedSourceCharacter {
        use VirtualControlElement::*;
        match self.control_element {
            Button(_) => ExtendedSourceCharacter::Normal(SourceCharacter::Button),
            Multi(_) => ExtendedSourceCharacter::VirtualContinuous,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct VirtualSourceValue {
    control_element: VirtualControlElement,
    control_value: ControlValue,
}

impl VirtualSourceValue {
    pub fn new(
        control_element: VirtualControlElement,
        control_value: ControlValue,
    ) -> VirtualSourceValue {
        VirtualSourceValue {
            control_element,
            control_value,
        }
    }

    pub fn control_element(&self) -> VirtualControlElement {
        self.control_element
    }

    pub fn control_value(&self) -> ControlValue {
        self.control_value
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum VirtualControlElement {
    Multi(VirtualControlElementId),
    Button(VirtualControlElementId),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum VirtualControlElementId {
    Indexed(u32),
    // No full String because we don't want heap allocations due to clones in real-time thread.
    Named(SmallAsciiString),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct SmallAsciiString {
    content: [u8; SmallAsciiString::MAX_LENGTH],
    length: u8,
}

impl SmallAsciiString {
    pub const MAX_LENGTH: usize = 16;

    pub fn create_compatible_ascii_string(text: &str) -> AsciiString {
        let fixed_text = text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
            .map(|c| c.to_ascii_char().unwrap());
        let ascii_string = AsciiString::from_iter(fixed_text);
        AsciiString::from(&ascii_string.as_slice()[..Self::MAX_LENGTH.min(ascii_string.len())])
    }

    pub fn from_ascii_str(ascii_str: &AsciiStr) -> Result<Self, &'static str> {
        if ascii_str.len() > SmallAsciiString::MAX_LENGTH {
            return Err("too large to be a small ASCII string");
        }
        let mut content = [0u8; SmallAsciiString::MAX_LENGTH];
        content[..ascii_str.len()].copy_from_slice(ascii_str.as_bytes());
        let res = Self {
            content,
            length: ascii_str.len() as u8,
        };
        Ok(res)
    }

    pub fn as_ascii_str(&self) -> &AsciiStr {
        AsciiStr::from_ascii(self.as_slice()).unwrap()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.content[..(self.length as usize)]
    }
}

impl Display for SmallAsciiString {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.as_ascii_str().fmt(f)
    }
}

impl Display for VirtualControlElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use VirtualControlElement::*;
        match self {
            Multi(id) => write!(f, "Multi {}", id),
            Button(id) => write!(f, "Button {}", id),
        }
    }
}

impl Display for VirtualControlElementId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use VirtualControlElementId::*;
        match self {
            Indexed(index) => write!(f, "{}", index + 1),
            Named(name) => name.fmt(f),
        }
    }
}

impl VirtualControlElement {
    pub fn id(&self) -> VirtualControlElementId {
        use VirtualControlElement::*;
        match self {
            Multi(i) | Button(i) => *i,
        }
    }
}
