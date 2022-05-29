mod playtime_clip_engine;

use crate::main::ClipSlotCoordinates;
use playtime_api::runtime::ClipPlayState;
pub use playtime_clip_engine::*;

impl SlotCoordinates {
    pub fn from_engine(coordinates: ClipSlotCoordinates) -> Self {
        Self {
            column: coordinates.column() as _,
            row: coordinates.row() as _,
        }
    }
}

impl SlotPlayState {
    pub fn from_engine(play_state: ClipPlayState) -> Self {
        use ClipPlayState::*;
        match play_state {
            Stopped => Self::Stopped,
            ScheduledForPlayStart => Self::ScheduledForPlayStart,
            Playing => Self::Playing,
            Paused => Self::Paused,
            ScheduledForPlayStop => Self::ScheduledForPlayStop,
            ScheduledForRecordingStart => Self::ScheduledForRecordingStart,
            Recording => Self::Recording,
            ScheduledForRecordingStop => Self::ScheduledForRecordingStop,
        }
    }
}
