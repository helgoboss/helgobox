use crate::base::default_util::is_default;
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
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub file_name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub preset_name: String,
}

impl fmt::Display for FxId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fn dash_if_empty(s: &str) -> &str {
            if s.is_empty() {
                "-"
            } else {
                s
            }
        }
        write!(
            f,
            "Name: {} | File: {} | Preset: {}",
            dash_if_empty(&self.name),
            dash_if_empty(&self.file_name),
            dash_if_empty(&self.preset_name)
        )
    }
}

impl FxId {
    pub fn from_fx(fx: &Fx, most_relevant_only: bool) -> Result<FxId, &'static str> {
        let fx_info = fx.info()?;
        let mut fx_id = FxId {
            name: fx_info.effect_name.trim().to_string(),
            ..Default::default()
        };
        if !fx_id.name.is_empty() && most_relevant_only {
            return Ok(fx_id);
        }
        fx_id.file_name = fx_info
            .file_name
            .to_str()
            .ok_or("invalid FX file name")?
            .trim()
            .to_string();
        if !fx_id.file_name.is_empty() && most_relevant_only {
            return Ok(fx_id);
        }
        fx_id.preset_name = fx
            .preset_name()
            .map(|s| s.to_str().trim().to_string())
            .unwrap_or_default();
        Ok(fx_id)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn has_name(&self) -> bool {
        !self.name.is_empty()
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn has_file_name(&self) -> bool {
        !self.file_name.is_empty()
    }

    pub fn preset_name(&self) -> &str {
        &self.preset_name
    }

    pub fn has_preset_name(&self) -> bool {
        !self.preset_name.is_empty()
    }

    /// Every field in the pattern that's filled must match!
    ///
    /// The pattern FX ID fields can contain wildcards.
    pub fn matches(&self, fx_id_pattern: &FxId) -> bool {
        if fx_id_pattern.has_name() && !self.name_matches(fx_id_pattern) {
            return false;
        }
        if fx_id_pattern.has_file_name() && !self.file_name_matches(fx_id_pattern) {
            return false;
        }
        if fx_id_pattern.has_preset_name() && !self.preset_name_matches(fx_id_pattern) {
            return false;
        }
        true
    }

    fn name_matches(&self, fx_id_pattern: &FxId) -> bool {
        let wild_match = wildmatch::WildMatch::new(&fx_id_pattern.name);
        wild_match.matches(self.name())
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
