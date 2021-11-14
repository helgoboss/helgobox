use derive_more::Display;
use helgoboss_midi::RawShortMessage;
use std::ops::RangeInclusive;

#[derive(PartialEq, Debug)]
pub struct Widget {
    pub name: String,
    pub capabilities: Vec<Capability>,
}

#[derive(PartialEq, Debug, Display)]
pub enum Capability {
    #[display(fmt = "Press")]
    Press {
        press: RawShortMessage,
        release: Option<RawShortMessage>,
    },
    #[display(fmt = "FB_TwoState")]
    FbTwoState {
        on: RawShortMessage,
        off: RawShortMessage,
    },
    #[display(fmt = "Encoder")]
    Encoder {
        main: RawShortMessage,
        accelerations: Option<Accelerations>,
    },
    #[display(fmt = "FB_Encoder")]
    FbEncoder { max: RawShortMessage },
    #[display(fmt = "Toggle")]
    Toggle { on: RawShortMessage },
    #[display(fmt = "Fader14Bit")]
    Fader14Bit { max: RawShortMessage },
    #[display(fmt = "FB_Fader14Bit")]
    FbFader14Bit { max: RawShortMessage },
    #[display(fmt = "Touch")]
    Touch {
        touch: RawShortMessage,
        release: RawShortMessage,
    },
    #[display(fmt = "FB_MCUDisplayLower")]
    FbMcuDisplayLower { index: u8 },
    #[display(fmt = "FB_MCUDisplayUpper")]
    FbMcuDisplayUpper { index: u8 },
    #[display(fmt = "FB_MCUTimeDisplay")]
    FbMcuTimeDisplay,
    #[display(fmt = "FB_MCUVUMeter")]
    FbMcuVuMeter { index: u8 },
    #[display(fmt = "{}", "_0")]
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
    pub decrements: Acceleration,
    pub increments: Acceleration,
}

#[derive(PartialEq, Debug)]
pub enum Acceleration {
    Sequence(Vec<u8>),
    Range(RangeInclusive<u8>),
}
