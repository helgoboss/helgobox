use reaper_medium::ReaperVolumeValue;
use serde::{Deserialize, Serialize};

use crate::main::ClipContent;

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

/// Describes settings and contents of one clip slot.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ClipData {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content")]
    pub content: ClipContent,
}
