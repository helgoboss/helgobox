use crate::domain::ui_util::{format_as_percentage_without_unit, parse_unit_value_from_percentage};
use crate::domain::{ExtendedSourceCharacter, TargetCharacter};
use ascii::{AsciiString, ToAsciiChar};
use base::SmallAsciiString;
use derivative::Derivative;
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, FeedbackValue, SourceCharacter, Target, UnitValue,
};
use helgobox_api::persistence::VirtualControlElementCharacter;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct VirtualTarget {
    pub control_element: VirtualControlElement,
    pub learnable: bool,
}

impl VirtualTarget {
    pub fn control_element(&self) -> VirtualControlElement {
        self.control_element
    }

    pub fn character(&self) -> TargetCharacter {
        use VirtualControlElementCharacter::*;
        match self.control_element.character() {
            Multi => TargetCharacter::VirtualMulti,
            Button => TargetCharacter::VirtualButton,
        }
    }
}

impl Target<'_> for VirtualTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        None
    }

    fn control_type(&self, _: ()) -> ControlType {
        use VirtualControlElementCharacter::*;
        match self.control_element.character() {
            Multi => ControlType::VirtualMulti,
            Button => ControlType::VirtualButton,
        }
    }
}

/// With virtual sources it's easy: The control element is the address.
pub type VirtualSourceAddress = VirtualControlElement;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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
    /// - Feedback diffing
    pub fn has_same_feedback_address_as_source(&self, other: &Self) -> bool {
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
        use VirtualControlElementCharacter::*;
        match self.control_element.character() {
            Button => ExtendedSourceCharacter::Normal(SourceCharacter::MomentaryButton),
            Multi => ExtendedSourceCharacter::VirtualContinuous,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct VirtualSourceValue {
    control_element: VirtualControlElement,
    control_value: ControlValue,
}

impl Display for VirtualSourceValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} with value {}",
            self.control_element, self.control_value
        )
    }
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

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct VirtualFeedbackValue {
    control_element: VirtualControlElement,
    feedback_value: FeedbackValue<'static>,
}

impl Display for VirtualFeedbackValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} with value {}",
            self.control_element, self.feedback_value
        )
    }
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

/// The combination of virtual control element ID and character.
///
/// When matching indexed (numbered) control elements, it makes a difference whether the character is a multi or
/// a button! The character is part of the identifier, so to say. The reason is that indexed control elements
/// were designed to model typical "8 knob & 8 buttons" controllers. In this case it's important to
/// consider knob 1 as a different control element than button 1.
///
/// For named control elements, the character doesn't act as an identifier, just as a hint for the UI optimizations.
/// Two named control elements are considered the same even they have 2 different characters. The rationale is that
/// named control elements have the freedom to use different names and should do so in order to avoid confusion.
///
/// Maybe it would be better to make indexed control elements behave like named ones (= ignore the character).
/// But we have to maintain backwards compatibility. Named control elements were added much later, so in the beginning
/// it was vital to distinguish between characters, and thus I suspect there are many presets still using this system.
#[derive(Copy, Clone, Ord, PartialOrd, Debug, Derivative)]
#[derivative(Eq, PartialEq, Hash)]
pub enum VirtualControlElement {
    Indexed {
        id: u32,
        character: VirtualControlElementCharacter,
    },
    Named {
        id: SmallAsciiString,
        #[derivative(PartialEq = "ignore", Hash = "ignore")]
        character: VirtualControlElementCharacter,
    },
}

impl VirtualControlElement {
    pub fn new(id: VirtualControlElementId, character: VirtualControlElementCharacter) -> Self {
        match id {
            VirtualControlElementId::Indexed(id) => Self::Indexed { id, character },
            VirtualControlElementId::Named(id) => Self::Named { id, character },
        }
    }

    pub fn id(&self) -> VirtualControlElementId {
        match self {
            VirtualControlElement::Indexed { id, .. } => VirtualControlElementId::Indexed(*id),
            VirtualControlElement::Named { id, .. } => VirtualControlElementId::Named(*id),
        }
    }

    pub fn character(&self) -> VirtualControlElementCharacter {
        match self {
            VirtualControlElement::Indexed { character, .. } => *character,
            VirtualControlElement::Named { character, .. } => *character,
        }
    }
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
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{} {}", self.character(), self.id())
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
            "ch1/v-pot/boost-cut",
            "ch2/v-pot/boost-cut",
            "ch3/v-pot/boost-cut",
            "ch4/v-pot/boost-cut",
            "ch5/v-pot/boost-cut",
            "ch6/v-pot/boost-cut",
            "ch7/v-pot/boost-cut",
            "ch8/v-pot/boost-cut",
            "ch1/v-pot/single",
            "ch2/v-pot/single",
            "ch3/v-pot/single",
            "ch4/v-pot/single",
            "ch5/v-pot/single",
            "ch6/v-pot/single",
            "ch7/v-pot/single",
            "ch8/v-pot/single",
            "ch1/v-pot/spread",
            "ch2/v-pot/spread",
            "ch3/v-pot/spread",
            "ch4/v-pot/spread",
            "ch5/v-pot/spread",
            "ch6/v-pot/spread",
            "ch7/v-pot/spread",
            "ch8/v-pot/spread",
            "jog",
            "lcd/assignment",
            "lcd/timecode",
            "ch1/lcd/line1",
            "ch1/lcd/line2",
            "ch2/lcd/line1",
            "ch2/lcd/line2",
            "ch3/lcd/line1",
            "ch3/lcd/line2",
            "ch4/lcd/line1",
            "ch4/lcd/line2",
            "ch5/lcd/line1",
            "ch5/lcd/line2",
            "ch6/lcd/line1",
            "ch6/lcd/line2",
            "ch7/lcd/line1",
            "ch7/lcd/line2",
            "ch8/lcd/line1",
            "ch8/lcd/line2",
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
    pub mod grid {
        pub const PREDEFINED_VIRTUAL_MULTI_NAMES: &[&str] = &[];
        pub const PREDEFINED_VIRTUAL_BUTTON_NAMES: &[&str] = &[
            "col1/stop",
            "col2/stop",
            "col3/stop",
            "col4/stop",
            "col5/stop",
            "col6/stop",
            "col7/stop",
            "col8/stop",
            "row1/play",
            "row2/play",
            "row3/play",
            "row4/play",
            "row5/play",
            "row6/play",
            "row7/play",
            "row8/play",
            "col1/row1/pad",
            "col1/row2/pad",
            "col1/row3/pad",
            "col1/row4/pad",
            "col1/row5/pad",
            "col1/row6/pad",
            "col1/row7/pad",
            "col1/row8/pad",
            "col2/row1/pad",
            "col2/row2/pad",
            "col2/row3/pad",
            "col2/row4/pad",
            "col2/row5/pad",
            "col2/row6/pad",
            "col2/row7/pad",
            "col2/row8/pad",
            "col3/row1/pad",
            "col3/row2/pad",
            "col3/row3/pad",
            "col3/row4/pad",
            "col3/row5/pad",
            "col3/row6/pad",
            "col3/row7/pad",
            "col3/row8/pad",
            "col4/row1/pad",
            "col4/row2/pad",
            "col4/row3/pad",
            "col4/row4/pad",
            "col4/row5/pad",
            "col4/row6/pad",
            "col4/row7/pad",
            "col4/row8/pad",
            "col5/row1/pad",
            "col5/row2/pad",
            "col5/row3/pad",
            "col5/row4/pad",
            "col5/row5/pad",
            "col5/row6/pad",
            "col5/row7/pad",
            "col5/row8/pad",
            "col6/row1/pad",
            "col6/row2/pad",
            "col6/row3/pad",
            "col6/row4/pad",
            "col6/row5/pad",
            "col6/row6/pad",
            "col6/row7/pad",
            "col6/row8/pad",
            "col7/row1/pad",
            "col7/row2/pad",
            "col7/row3/pad",
            "col7/row4/pad",
            "col7/row5/pad",
            "col7/row6/pad",
            "col7/row7/pad",
            "col7/row8/pad",
            "col8/row1/pad",
            "col8/row2/pad",
            "col8/row3/pad",
            "col8/row4/pad",
            "col8/row5/pad",
            "col8/row6/pad",
            "col8/row7/pad",
            "col8/row8/pad",
            "shift",
            "click",
            "undo",
            "delete",
            "quantize",
            "duplicate",
            "double",
            "record",
            "record-arm",
            "track-select",
            "mute",
            "solo",
            "volume",
            "pan",
            "sends",
            "stop-clip",
            "cursor-up",
            "cursor-down",
            "cursor-left",
            "cursor-right",
            "session",
            "note",
            "device",
            "user",
            // Found this one on the APC Key 25
            "stop-all-clips",
        ];
    }
}
