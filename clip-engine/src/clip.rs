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
pub struct LegacyClip {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content", default, skip_serializing_if = "is_default")]
    pub content: Option<ClipContent>,
}

impl Default for LegacyClip {
    fn default() -> Self {
        Self {
            volume: ReaperVolumeValue::ZERO_DB,
            repeat: false,
            content: None,
        }
    }
}

impl LegacyClip {
    pub fn is_filled(&self) -> bool {
        self.content.is_some()
    }
}

/// Play state of a clip.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ClipPlayState {
    Stopped,
    ScheduledForPlay,
    Playing,
    Paused,
    ScheduledForStop,
    Recording,
}

impl ClipPlayState {
    /// Translates this play state into a feedback value.
    pub fn feedback_value(self) -> UnitValue {
        use ClipPlayState::*;
        match self {
            Stopped => UnitValue::MIN,
            ScheduledForPlay => UnitValue::new(0.75),
            Playing => UnitValue::MAX,
            Paused => UnitValue::new(0.5),
            ScheduledForStop => UnitValue::new(0.25),
            Recording => UnitValue::new(0.60),
        }
    }
}

impl Default for ClipPlayState {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug)]
pub enum ClipChangedEvent {
    PlayState(ClipPlayState),
    ClipVolume(ReaperVolumeValue),
    ClipRepeat(bool),
    ClipPosition(UnitValue),
}
