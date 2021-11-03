use helgoboss_midi::RawShortMessage;
use std::ops::RangeInclusive;

#[derive(PartialEq, Debug)]
pub struct Widget {
    pub name: String,
    pub capabilities: Vec<Capability>,
}

#[derive(PartialEq, Debug)]
pub enum Capability {
    Press {
        press: RawShortMessage,
        release: Option<RawShortMessage>,
    },
    FbTwoState {
        on: RawShortMessage,
        off: RawShortMessage,
    },
    Encoder {
        main: RawShortMessage,
        accelerations: Option<Accelerations>,
    },
    FbEncoder {
        max: RawShortMessage,
    },
    Toggle {
        on: RawShortMessage,
    },
    Fader14Bit {
        max: RawShortMessage,
    },
    FbFader14Bit {
        max: RawShortMessage,
    },
    Touch {
        on: RawShortMessage,
        off: RawShortMessage,
    },
    FbMcuDisplayLower {
        index: u32,
    },
    FbMcuDisplayUpper {
        index: u32,
    },
    FbMcuTimeDisplay,
    FbMcuVuMeter {
        index: u32,
    },
    Unknown(String),
}

impl Capability {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }

    pub fn is_virtual_button(&self) -> bool {
        use Capability::*;
        matches!(self, Press { .. } | Toggle { .. } | Touch { .. })
    }
}

#[derive(PartialEq, Debug)]
pub struct Accelerations {
    pub increments: Acceleration,
    pub decrements: Acceleration,
}

#[derive(PartialEq, Debug)]
pub enum Acceleration {
    Sequence(Vec<u8>),
    Range(RangeInclusive<u8>),
}
