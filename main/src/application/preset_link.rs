use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

use derive_more::Display;
use reaper_high::Fx;
use smallvec::alloc::fmt::Formatter;
use std::fmt;

pub trait PresetLinkManager: fmt::Debug {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String>;
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct FxId {
    file_name: String,
}

impl fmt::Display for FxId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file_name)
    }
}

impl FxId {
    pub fn new(file_name: String) -> FxId {
        FxId { file_name }
    }

    pub fn from_fx(fx: &Fx) -> FxId {
        let fx_info = fx.info();
        let file_name = fx_info.file_name.to_str().expect("invalid FX file name");
        FxId {
            file_name: file_name.to_string(),
        }
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }
}

#[derive(
    Copy,
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum MainPresetAutoLoadMode {
    #[serde(rename = "off")]
    #[display(fmt = "Off")]
    Off,
    #[serde(rename = "focused-fx")]
    #[display(fmt = "Depending on focused FX")]
    FocusedFx,
}

impl Default for MainPresetAutoLoadMode {
    fn default() -> Self {
        Self::Off
    }
}
