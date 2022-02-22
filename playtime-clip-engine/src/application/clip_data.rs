use std::error::Error;
use std::path::{Path, PathBuf};

use reaper_high::{Item, OwnedSource, Project, Reaper, ReaperSource};
use reaper_medium::{MidiImportBehavior, ReaperVolumeValue};
use serde::{Deserialize, Serialize};

use crate::ClipContent;
use helgoboss_learn::UnitValue;

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
