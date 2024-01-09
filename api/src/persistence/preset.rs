use crate::util::deserialize_null_default;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Meta data that is common to both main and controller presets.
///
/// Preset meta data is everything that is loaded right at startup in order to be able to
/// display a list of preset, do certain validations etc. It doesn't include the preset
/// content which is necessary to actually use the preset (e.g. it doesn't include the mappings).
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct CommonPresetMetaData {
    /// Display name of the preset.
    pub name: String,
    /// The ReaLearn version for which this preset was built.
    ///
    /// This can effect the way the preset is loaded, e.g. it can lead to different interpretation
    /// or migration of properties. So care should be taken to set this correctly!
    ///
    /// If `None`, it's assumed that it was built for a very old version (< 1.12.0-pre18) that
    /// didn't have the versioning concept yet.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(alias = "version")]
    pub realearn_version: Option<Version>,
}

/// Meta data that is specific to controller presets.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ControllerPresetMetaData {
    #[serde(default)]
    pub provided_schemes: HashSet<VirtualControlSchemeId>,
}

/// Meta data that is specific to main presets.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MainPresetMetaData {
    #[serde(default)]
    pub used_schemes: HashSet<VirtualControlSchemeId>,
    #[serde(default)]
    pub provided_roles: HashSet<String>,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct VirtualControlSchemeId(String);

impl VirtualControlSchemeId {
    pub fn get(&self) -> &str {
        &self.0
    }
}
