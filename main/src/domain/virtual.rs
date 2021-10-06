use crate::domain::ui_util::{format_as_percentage_without_unit, parse_unit_value_from_percentage};
use crate::domain::{ExtendedSourceCharacter, SmallAsciiString, TargetCharacter};
use ascii::{AsciiString, ToAsciiChar};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, FeedbackValue, SourceCharacter, Target, UnitValue,
};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::fmt::Display;
use std::str::FromStr;

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

impl<'a> Target<'a> for VirtualTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, _: ()) -> ControlType {
        use VirtualControlElement::*;
        match self.control_element {
            Multi(_) => ControlType::VirtualMulti,
            Button(_) => ControlType::VirtualButton,
        }
    }
}

/// With virtual sources it's easy: The control element is the address.
pub type VirtualSourceAddress = VirtualControlElement;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct VirtualSource {
    control_element: VirtualControlElement,
}

impl VirtualSource {
    pub fn feedback_address(&self) -> &VirtualSourceAddress {
        &self.control_element
    }

    /// Checks if this and the given source share the same address.
    ///
    /// Used for:
    ///
    /// - Source filtering
    /// - Feedback diffing
    pub fn source_address_matches(&self, other: &Self) -> bool {
        self.control_element == other.control_element
    }

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

    pub fn feedback(&self, feedback_value: FeedbackValue) -> VirtualFeedbackValue {
        VirtualFeedbackValue::new(self.control_element, feedback_value.make_owned())
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        let absolute_value = value.to_unit_value()?;
        Ok(format_as_percentage_without_unit(absolute_value))
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        parse_unit_value_from_percentage(text)
    }

    pub fn character(&self) -> ExtendedSourceCharacter {
        use VirtualControlElement::*;
        match self.control_element {
            Button(_) => ExtendedSourceCharacter::Normal(SourceCharacter::MomentaryButton),
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

#[derive(Clone, PartialEq, Debug)]
pub struct VirtualFeedbackValue {
    control_element: VirtualControlElement,
    feedback_value: FeedbackValue<'static>,
}

impl VirtualFeedbackValue {
    pub fn new(
        control_element: VirtualControlElement,
        feedback_value: FeedbackValue<'static>,
    ) -> Self {
        VirtualFeedbackValue {
            control_element,
            feedback_value,
        }
    }

    pub fn control_element(&self) -> VirtualControlElement {
        self.control_element
    }

    pub fn feedback_value(&self) -> &FeedbackValue {
        &self.feedback_value
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub enum VirtualControlElement {
    Multi(VirtualControlElementId),
    Button(VirtualControlElementId),
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub enum VirtualControlElementId {
    Indexed(u32),
    // No full String because we don't want heap allocations due to clones in real-time thread.
    Named(SmallAsciiString),
}

impl FromStr for VirtualControlElementId {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Ok(position) = text.parse::<i32>() {
            let index = std::cmp::max(0, position - 1) as u32;
            Ok(Self::Indexed(index))
        } else {
            let small_ascii_string = create_control_element_name_lossy(text)?;
            Ok(Self::Named(small_ascii_string))
        }
    }
}

/// Keeps only alphanumeric and punctuation ASCII characters and crops the string if too long.
fn create_control_element_name_lossy(text: &str) -> Result<SmallAsciiString, &'static str> {
    let ascii_string: AsciiString = text
        .chars()
        .filter_map(|c| c.to_ascii_char().ok())
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation())
        .collect();
    if ascii_string.is_empty() {
        return Err("empty virtual control element name");
    }
    Ok(SmallAsciiString::from_ascii_str_cropping(&ascii_string))
}

impl Default for VirtualControlElementId {
    fn default() -> Self {
        Self::Indexed(0)
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

pub mod control_element_domains {
    pub mod daw {
        pub const PREDEFINED_VIRTUAL_MULTI_NAMES: &[&str] = &[
            "main/fader",
            "ch1/fader",
            "ch2/fader",
            "ch3/fader",
            "ch4/fader",
            "ch5/fader",
            "ch6/fader",
            "ch7/fader",
            "ch8/fader",
            "ch1/v-pot",
            "ch2/v-pot",
            "ch3/v-pot",
            "ch4/v-pot",
            "ch5/v-pot",
            "ch6/v-pot",
            "ch7/v-pot",
            "ch8/v-pot",
            "jog",
            "lcd/assignment",
        ];

        pub const PREDEFINED_VIRTUAL_BUTTON_NAMES: &[&str] = &[
            "ch1/v-select",
            "ch2/v-select",
            "ch3/v-select",
            "ch4/v-select",
            "ch5/v-select",
            "ch6/v-select",
            "ch7/v-select",
            "ch8/v-select",
            "ch1/select",
            "ch2/select",
            "ch3/select",
            "ch4/select",
            "ch5/select",
            "ch6/select",
            "ch7/select",
            "ch8/select",
            "ch1/mute",
            "ch2/mute",
            "ch3/mute",
            "ch4/mute",
            "ch5/mute",
            "ch6/mute",
            "ch7/mute",
            "ch8/mute",
            "ch1/solo",
            "ch2/solo",
            "ch3/solo",
            "ch4/solo",
            "ch5/solo",
            "ch6/solo",
            "ch7/solo",
            "ch8/solo",
            "ch1/record-ready",
            "ch2/record-ready",
            "ch3/record-ready",
            "ch4/record-ready",
            "ch5/record-ready",
            "ch6/record-ready",
            "ch7/record-ready",
            "ch8/record-ready",
            "main/fader/touch",
            "ch1/fader/touch",
            "ch2/fader/touch",
            "ch3/fader/touch",
            "ch4/fader/touch",
            "ch5/fader/touch",
            "ch6/fader/touch",
            "ch7/fader/touch",
            "ch8/fader/touch",
            "marker",
            "read",
            "write",
            "rewind",
            "fast-fwd",
            "play",
            "stop",
            "record",
            "cycle",
            "zoom",
            "scrub",
            "nudge",
            "drop",
            "replace",
            "click",
            "solo",
            "f1",
            "f2",
            "f3",
            "f4",
            "f5",
            "f6",
            "f7",
            "f8",
            "smpte-beats",
            // Chose to make the following buttons, not multis - although ReaLearn would allow to
            // convert them into multis in the virtual controller mapping. Reason: On
            // Mackie consoles these are usually buttons. Exposing them as buttons has
            // the benefit that we can use Realearn's button-specific features in the
            // main mapping such as advanced fire modes.
            "ch-left",
            "ch-right",
            "bank-left",
            "bank-right",
            "cursor-left",
            "cursor-right",
            "cursor-up",
            "cursor-down",
        ];
    }
}
