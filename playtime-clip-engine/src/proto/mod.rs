mod playtime_clip_engine;

use crate::main::ClipSlotCoordinates;
use playtime_api::runtime::ClipPlayState;
pub use playtime_clip_engine::*;
use reaper_medium::{InputMonitoringMode, RecordingInput, RgbColor};

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

impl TrackColor {
    pub fn from_engine(color: Option<RgbColor>) -> Self {
        Self {
            color: color
                .map(|c| (((c.r as u32) << 16) + ((c.g as u32) << 8) + (c.b as u32)) as i32),
        }
    }
}

impl TrackInput {
    pub fn from_engine(input: Option<RecordingInput>) -> Self {
        use track_input::Input;
        use RecordingInput::*;
        let input = match input {
            Some(Mono(ch)) => Some(Input::Mono(ch)),
            Some(Stereo(ch)) => Some(Input::Stereo(ch)),
            Some(Midi { device_id, channel }) => {
                let midi_input = TrackMidiInput {
                    device: device_id.map(|id| id.get() as _),
                    channel: channel.map(|ch| ch.get() as _),
                };
                Some(Input::Midi(midi_input))
            }
            _ => None,
        };
        Self { input }
    }
}

impl TrackInputMonitoring {
    pub fn from_engine(mode: InputMonitoringMode) -> Self {
        match mode {
            InputMonitoringMode::Off => Self::Off,
            InputMonitoringMode::Normal => Self::Normal,
            InputMonitoringMode::NotWhenPlaying => Self::TapeStyle,
            InputMonitoringMode::Unknown(_) => Self::Unknown,
        }
    }
}
