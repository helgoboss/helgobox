use crate::domain::{OscDeviceId, StreamDeckDeviceId};
use derivative::Derivative;
use helgoboss_learn::{ControlValue, FeedbackValue, RgbColor, UnitValue};
use helgobox_api::persistence::StreamDeckButtonDesign;
use rosc::OscMessage;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct StreamDeckSource {
    pub button_index: u32,
    pub button_design: StreamDeckButtonDesign,
}

impl StreamDeckSource {
    pub fn new(button_index: u32, button_design: StreamDeckButtonDesign) -> Self {
        Self {
            button_index,
            button_design,
        }
    }

    pub fn feedback_address(&self) -> StreamDeckSourceAddress {
        StreamDeckSourceAddress {
            button_index: self.button_index,
        }
    }

    /// Checks if the given message is directed to the same address as the one of this source.
    ///
    /// Used for:
    ///
    /// -  Source takeover (feedback)
    pub fn has_same_feedback_address_as_value(
        &self,
        value: &StreamDeckSourceFeedbackValue,
    ) -> bool {
        self.feedback_address() == value.feedback_address()
    }

    /// Checks if this and the given source share the same address.
    ///
    /// Used for:
    ///
    /// - Feedback diffing
    pub fn has_same_feedback_address_as_source(&self, other: &Self) -> bool {
        self.feedback_address() == other.feedback_address()
    }

    pub fn control(&self, msg: StreamDeckMessage) -> Option<ControlValue> {
        if msg.button_index != self.button_index {
            return None;
        }
        let val = if msg.press {
            UnitValue::MAX
        } else {
            UnitValue::MIN
        };
        Some(ControlValue::AbsoluteContinuous(val))
    }

    pub fn feedback(
        &self,
        feedback_value: &FeedbackValue,
    ) -> Option<StreamDeckSourceFeedbackValue> {
        let value = match feedback_value {
            FeedbackValue::Off => StreamDeckSourceFeedbackValue {
                button_index: self.button_index,
                button_design: self.button_design.clone(),
                background_color: Some(RgbColor::BLACK),
                foreground_color: None,
                numeric_value: None,
                text_value: None,
            },
            FeedbackValue::Numeric(v) => StreamDeckSourceFeedbackValue {
                button_index: self.button_index,
                button_design: self.button_design.clone(),
                background_color: v.style.background_color,
                foreground_color: v.style.color,
                numeric_value: Some(v.value.to_unit_value()),
                text_value: None,
            },
            FeedbackValue::Textual(v) => StreamDeckSourceFeedbackValue {
                button_index: self.button_index,
                button_design: self.button_design.clone(),
                background_color: v.style.background_color,
                foreground_color: v.style.color,
                numeric_value: None,
                text_value: Some(v.text.to_string()),
            },
            FeedbackValue::Complex(_) => {
                // TODO-high CONTINUE supporting complex dynamically generated feedback (by glue section)
                return None;
            }
        };
        Some(value)
    }
}

impl Display for StreamDeckSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Button {}", self.button_index + 1)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct StreamDeckMessage {
    pub button_index: u32,
    pub press: bool,
}

impl StreamDeckMessage {
    pub fn new(button_index: u32, press: bool) -> Self {
        Self {
            button_index,
            press,
        }
    }
}

impl Display for StreamDeckMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.button_index, self.press)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Derivative)]
#[derivative(Hash)]
pub struct StreamDeckSourceFeedbackValue {
    pub button_index: u32,
    pub button_design: StreamDeckButtonDesign,
    pub background_color: Option<RgbColor>,
    pub foreground_color: Option<RgbColor>,
    #[derivative(Hash(hash_with = "hash_opt_unit_value_for_change_detection"))]
    pub numeric_value: Option<UnitValue>,
    pub text_value: Option<String>,
}

fn hash_opt_unit_value_for_change_detection<H>(value: &Option<UnitValue>, state: &mut H)
where
    H: Hasher,
{
    let raw = value.map(|v| v.get().to_ne_bytes());
    raw.hash(state);
}

impl StreamDeckSourceFeedbackValue {
    pub fn feedback_address(&self) -> StreamDeckSourceAddress {
        StreamDeckSourceAddress {
            button_index: self.button_index,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct StreamDeckSourceAddress {
    pub button_index: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct StreamDeckScanResult {
    pub message: StreamDeckMessage,
    pub dev_id: Option<StreamDeckDeviceId>,
}
