use crate::domain::ui_util::{format_as_percentage_without_unit, parse_from_percentage};
use crate::domain::TargetCharacter;
use helgoboss_learn::{ControlType, ControlValue, SourceCharacter, UnitValue};
use smallvec::alloc::fmt::Formatter;
use std::fmt::Display;

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

    pub fn control_type(&self) -> ControlType {
        ControlType::Virtual
    }

    pub fn character(&self) -> TargetCharacter {
        use VirtualControlElement::*;
        match self.control_element {
            Continuous(_) => TargetCharacter::VirtualContinuous,
            Button(_) => TargetCharacter::VirtualButton,
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
        parse_from_percentage(text)
    }

    pub fn character(&self) -> SourceCharacter {
        use VirtualControlElement::*;
        match self.control_element {
            // TODO-high This is not accurate. It's either range or a type of encoder.
            // Anyway, this is just for auto-correction of modes. We are going to use virtual
            // control elements with a new automatic mode probably, so this shouldn't matter.
            Continuous(_) => SourceCharacter::Range,
            Button(_) => SourceCharacter::Button,
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
    Continuous(u32),
    Button(u32),
}

impl Display for VirtualControlElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use VirtualControlElement::*;
        match self {
            Continuous(i) => write!(f, "Continuous {}", i + 1),
            Button(i) => write!(f, "Button {}", i + 1),
        }
    }
}

impl VirtualControlElement {
    pub fn index(&self) -> u32 {
        use VirtualControlElement::*;
        match self {
            Continuous(i) => *i,
            Button(i) => *i,
        }
    }
}
