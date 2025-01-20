use base::default_util::is_default;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

use derive_more::Display;
use reaper_high::{Fx, FxInfo};
use reaper_medium::ReaperStr;
use std::fmt;
use std::fmt::Formatter;
use strum::EnumIter;

pub trait PresetLinkManager: fmt::Debug {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String>;
}

pub trait PresetLinkMutator {
    fn update_fx_id(&mut self, old_fx_id: FxId, new_fx_id: FxId);

    fn remove_link(&mut self, fx_id: &FxId);

    fn link_preset_to_fx(&mut self, preset_id: String, fx_id: FxId);
}

#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxPresetLinkConfig {
    links: Vec<FxPresetLink>,
}

impl PresetLinkManager for FxPresetLinkConfig {
    fn find_preset_linked_to_fx(&self, fx_id: &FxId) -> Option<String> {
        // Let the links with preset name have precedence.
        find_match(
            self.links.iter().filter(|l| l.fx_id.has_preset_name()),
            fx_id,
        )
        .or_else(|| {
            find_match(
                self.links.iter().filter(|l| !l.fx_id.has_preset_name()),
                fx_id,
            )
        })
    }
}

impl PresetLinkMutator for FxPresetLinkConfig {
    fn update_fx_id(&mut self, old_fx_id: FxId, new_fx_id: FxId) {
        for link in &mut self.links {
            if link.fx_id == old_fx_id {
                link.fx_id = new_fx_id;
                return;
            }
        }
    }

    fn remove_link(&mut self, fx_id: &FxId) {
        self.links.retain(|l| &l.fx_id != fx_id);
    }

    fn link_preset_to_fx(&mut self, preset_id: String, fx_id: FxId) {
        let link = FxPresetLink { fx_id, preset_id };
        if let Some(l) = self.links.iter_mut().find(|l| l.fx_id == link.fx_id) {
            *l = link;
        } else {
            self.links.push(link);
        }
    }
}

impl FxPresetLinkConfig {
    pub fn links(&self) -> impl ExactSizeIterator<Item = &FxPresetLink> + '_ {
        self.links.iter()
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxPresetLink {
    #[serde(rename = "fx")]
    pub fx_id: FxId,
    #[serde(rename = "presetId")]
    pub preset_id: String,
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
    pub fn from_fx(fx: &Fx, most_relevant_only: bool) -> anyhow::Result<FxId> {
        let fx_info = fx.info()?;
        let preset_name = fx.preset_name();
        let fx_id = Self::from_fx_info_and_preset_name(
            &fx_info,
            preset_name.as_deref(),
            most_relevant_only,
        );
        Ok(fx_id)
    }

    pub fn from_fx_info_and_preset_name(
        fx_info: &FxInfo,
        preset_name: Option<&ReaperStr>,
        most_relevant_only: bool,
    ) -> FxId {
        let mut fx_id = FxId {
            name: fx_info.effect_name.trim().to_string(),
            ..Default::default()
        };
        if !fx_id.name.is_empty() && most_relevant_only {
            return fx_id;
        }
        fx_id.file_name = fx_info.file_name.to_string_lossy().trim().to_string();
        if !fx_id.file_name.is_empty() && most_relevant_only {
            return fx_id;
        }
        fx_id.preset_name = preset_name
            .map(|s| s.to_str().trim().to_string())
            .unwrap_or_default();
        fx_id
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
    Eq,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    EnumIter,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum AutoLoadMode {
    #[serde(rename = "off")]
    #[display(fmt = "Off")]
    Off,
    #[serde(rename = "focused-fx")]
    #[display(fmt = "Based on unit FX")]
    UnitFx,
}

impl Default for AutoLoadMode {
    fn default() -> Self {
        Self::Off
    }
}

impl AutoLoadMode {
    pub fn is_on(&self) -> bool {
        *self != Self::Off
    }
}

fn find_match<'a>(
    mut links: impl Iterator<Item = &'a FxPresetLink>,
    fx_id: &FxId,
) -> Option<String> {
    links.find_map(|link| {
        if fx_id.matches(&link.fx_id) {
            Some(link.preset_id.clone())
        } else {
            None
        }
    })
}
