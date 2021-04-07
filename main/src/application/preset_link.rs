use crate::core::default_util::is_default;
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

#[derive(Clone, Eq, PartialEq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxId {
    file_name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    preset_name: String,
}

impl fmt::Display for FxId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.preset_name.is_empty() {
            write!(f, "{}", self.file_name)
        } else {
            write!(f, "{} / {}", self.file_name, self.preset_name)
        }
    }
}

impl FxId {
    pub fn new(file_name: String, preset_name: String) -> FxId {
        FxId {
            file_name,
            preset_name,
        }
    }

    pub fn from_fx(fx: &Fx, complete: bool) -> Result<FxId, &'static str> {
        let fx_info = fx.info()?;
        let file_name = fx_info.file_name.to_str().ok_or("invalid FX file name")?;
        let id = FxId {
            file_name: file_name.to_string(),
            preset_name: if complete {
                fx.preset_name()
                    .map(|s| s.into_string())
                    .unwrap_or_default()
            } else {
                String::new()
            },
        };
        Ok(id)
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn preset_name(&self) -> &str {
        &self.preset_name
    }

    pub fn has_preset_name(&self) -> bool {
        !self.preset_name.is_empty()
    }

    /// The pattern FX ID can contain wildcards.
    pub fn matches(&self, fx_id_pattern: &FxId) -> bool {
        self.file_name_matches(fx_id_pattern)
            && (!fx_id_pattern.has_preset_name() || self.preset_name_matches(fx_id_pattern))
    }

    fn file_name_matches(&self, fx_id_pattern: &FxId) -> bool {
        let wild_match = wildmatch::WildMatch::new(&fx_id_pattern.file_name);
        wild_match.matches(self.file_name())
    }

    fn preset_name_matches(&self, fx_id_pattern: &FxId) -> bool {
        let wild_match = wildmatch::WildMatch::new(&fx_id_pattern.preset_name);
        wild_match.matches(self.preset_name())
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
