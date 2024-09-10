use crate::util::deserialize_null_default;
use semver::Version;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use strum::{Display, EnumString};

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
    /// Author of the preset.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub author: Option<String>,
    /// Preset description (prose).
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    /// Preset setup instructions (prose).
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub setup_instructions: Option<String>,
}

/// Meta data that is specific to controller presets.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ControllerPresetMetaData {
    /// Device manufacturer.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub device_manufacturer: Option<String>,
    /// Original name of the device.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub device_name: Option<String>,
    /// MIDI identity compatibility pattern.
    ///
    /// Will be used for auto-adding controllers and for finding the correct controller preset when calculating auto
    /// units.
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub midi_identity_pattern: Option<String>,
    /// Possible MIDI identity compatibility patterns.
    ///
    /// Will be used for auto-adding controllers and for finding the correct controller preset when calculating auto
    /// units.
    ///
    /// It should only be provided if the device in question doesn't reply to device queries or if it exposes
    /// multiple ports which all respond with the same device identity and only one of the ports is the correct one.
    /// Example: APC Key 25 mk2, which exposes a "Control" and a "Keys" port.
    ///
    /// It's a list because names often differ between operating systems. ReaLearn will match any in the list.
    #[serde(default)]
    pub midi_output_port_patterns: Vec<MidiPortPattern>,
    /// Provided virtual control schemes.
    ///
    /// Will be used for finding the correct controller preset when calculating auto units.
    ///
    /// The order matters! It directly influences the choice of the best-suited main presets. In particular,
    /// schemes that are more specific to this particular controller (e.g. "novation/launchpad-mk3") should come first.
    /// Generic schemes (e.g. "grid") should come last. When auto-picking a main preset, matches of more specific
    /// schemes will be favored over less specific ones.
    #[serde(default)]
    pub provided_schemes: Vec<VirtualControlSchemeId>,
}

#[derive(Clone, Eq, PartialEq, Debug, SerializeDisplay, DeserializeFromStr)]
pub struct MidiPortPattern {
    pub scope: Option<MidiPortPatternScope>,
    pub name_pattern: String,
}

impl MidiPortPattern {
    pub fn scope_matches(&self) -> bool {
        let Some(scope) = self.scope else {
            return true;
        };
        match scope {
            MidiPortPatternScope::Windows => cfg!(windows),
            MidiPortPatternScope::MacOs => cfg!(target_os = "macos"),
            MidiPortPatternScope::Linux => cfg!(target_os = "linux"),
        }
    }
}

impl FromStr for MidiPortPattern {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((scope_string, name_pattern)) = s.split_once(':') {
            if let Ok(scope) = MidiPortPatternScope::from_str(scope_string) {
                // MIDI port pattern with scope restriction
                let pattern = Self {
                    scope: Some(scope),
                    name_pattern: name_pattern.to_string(),
                };
                return Ok(pattern);
            }
        }
        // MIDI port pattern without scope restriction
        let pattern = Self {
            scope: None,
            name_pattern: s.to_string(),
        };
        Ok(pattern)
    }
}

impl Display for MidiPortPattern {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(s) = self.scope {
            write!(f, "{s}:")?;
        }
        self.name_pattern.fmt(f)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum MidiPortPatternScope {
    Windows,
    MacOs,
    Linux,
}

/// Metadata that is specific to main presets.
#[derive(Clone, Eq, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MainPresetMetaData {
    /// Used virtual control schemes.
    ///
    /// Will be used for finding the correct controller preset when calculating auto units.
    #[serde(default)]
    pub used_schemes: HashSet<VirtualControlSchemeId>,
    /// A set of features that a Helgobox instance needs to provide for the preset to make sense.
    ///
    /// See [instance_features].
    ///
    /// Will be used for determining whether an auto unit should be created for a specific instance
    /// or not. Example: If the required feature is "playtime" and a controller is configured with
    /// this main preset but the instance doesn't contain a Playtime Clip Matrix, this instance will
    /// not load the main preset.
    #[serde(default)]
    pub required_features: HashSet<String>,
}

impl MainPresetMetaData {
    pub fn requires_playtime(&self) -> bool {
        self.required_features.contains(instance_features::PLAYTIME)
    }

    /// Higher specificity means that the main preset uses a scheme provided by the controller that's more specific to
    /// that particular controller (and therefore better suited).
    ///
    /// When picking the "best" main preset for a given controller preset, this is the first criteria taken into
    /// account, if there are two competing main preset candidates.
    ///
    /// Given a controller preset that provides the schemes [bla, foo].
    /// If main preset A uses schemes [bla] and main preset B [foo], we want main preset A to win because
    /// "bla" comes first in the controller preset's list of provided schemes, meaning that "bla" is the
    /// more specific scheme.
    ///
    /// An example where this matters in practice:
    /// - Controller preset "Launchpad Pro mk3 - Live mode" provides schemes [novation/launchpad-pro-mk3/live, grid]
    /// - Main preset "Generic grid controller - Playtime" uses schemes [grid]
    /// - Main preset "Launchpad Pro mk3 - Playtime" uses schemes [novation/launchpad-pro-mk3/live]
    ///
    /// Without that rule, it could easily happen that "Generic grid controller - Playtime" will be picked. Bad!
    ///
    /// Returns `None` if no scheme matches.
    pub fn calc_scheme_specificity(
        &self,
        provided_schemes: &[VirtualControlSchemeId],
    ) -> Option<u8> {
        let lowest_matching_index = self
            .used_schemes
            .iter()
            .filter_map(|used_scheme| provided_schemes.iter().position(|s| s == used_scheme))
            .min()?;
        Some((provided_schemes.len() - lowest_matching_index) as u8)
    }

    /// Higher coverage means that the main preset uses more schemes provided by the controller.
    ///
    /// When picking the "best" main preset for a given controller preset, this is the second criteria taken into
    /// account, if the specificity of two main preset candidates is the same.
    pub fn calc_scheme_coverage(&self, provided_schemes: &[VirtualControlSchemeId]) -> u8 {
        self.used_schemes
            .iter()
            .filter(|used_scheme| provided_schemes.contains(used_scheme))
            .count() as u8
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct VirtualControlSchemeId(String);

impl VirtualControlSchemeId {
    pub fn get(&self) -> &str {
        &self.0
    }
}

/// Known instance features.
pub mod instance_features {
    /// Instance owns a Playtime Clip Matrix.
    pub const PLAYTIME: &str = "playtime";
}
