use derive_more::Display;
use serde::{Deserialize, Serialize};
use serde_with::DeserializeFromStr;
use std::str::FromStr;

/// An OSC device ID.
///
/// This uniquely identifies an OSC device according to ReaLearn's device configuration.
#[derive(
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    Default,
    Display,
    Serialize,
    DeserializeFromStr,
)]
pub struct OscDeviceId(String);

impl FromStr for OscDeviceId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("OSC device ID must not be empty");
        }
        let valid_regex = regex!(r#"^[a-z0-9-]+$"#);
        if valid_regex.is_match(trimmed) {
            return Err("OSC device must contain lowercase letters, digits and hyphens only");
        }
        Ok(OscDeviceId(trimmed.to_owned()))
    }
}
