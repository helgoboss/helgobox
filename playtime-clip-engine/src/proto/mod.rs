mod clip_engine;

use crate::base::{Clip, ClipSlotAddress, History, Matrix, Slot};
use crate::rt::InternalClipPlayState;
use crate::{base, clip_timeline, ClipEngineResult, Timeline};
pub use clip_engine::*;
use playtime_api::runtime::ClipPlayState;
use reaper_high::Project;
use reaper_medium::{
    Bpm, Db, InputMonitoringMode, PlayState, ReaperPanValue, RecordingInput, RgbColor,
};

impl occasional_matrix_update::Update {
    pub fn volume(db: Db) -> Self {
        Self::Volume(db.get())
    }

    pub fn pan(pan: ReaperPanValue) -> Self {
        Self::Pan(pan.get())
    }

    pub fn tempo(bpm: Bpm) -> Self {
        Self::Tempo(bpm.get())
    }

    pub fn arrangement_play_state(play_state: PlayState) -> Self {
        Self::ArrangementPlayState(ArrangementPlayState::from_engine(play_state).into())
    }

    pub fn complete_persistent_data(matrix: &Matrix) -> Self {
        let matrix_json =
            serde_json::to_string(&matrix.save()).expect("couldn't represent matrix as JSON");
        Self::CompletePersistentData(matrix_json)
    }

    pub fn history_state(matrix: &Matrix) -> Self {
        Self::HistoryState(HistoryState::from_engine(matrix.history()))
    }

    pub fn time_signature(project: Project) -> Self {
        Self::TimeSignature(TimeSignature::from_engine(project))
    }
}

impl clip_engine::TrackInList {
    pub fn from_engine(track: reaper_high::Track, level: u32) -> Self {
        Self {
            id: track.guid().to_string_without_braces(),
            name: track.name().unwrap_or_default().into_string(),
            level,
        }
    }
}

impl TimeSignature {
    pub fn from_engine(project: Project) -> Self {
        let timeline = clip_timeline(Some(project), true);
        let time_signature = timeline.time_signature_at(timeline.cursor_pos());
        TimeSignature {
            numerator: time_signature.numerator.get(),
            denominator: time_signature.denominator.get(),
        }
    }
}

impl occasional_track_update::Update {
    pub fn name(track: &reaper_high::Track) -> Self {
        Self::Name(track.name().unwrap_or_default().into_string())
    }

    pub fn color(track: &reaper_high::Track) -> Self {
        Self::Color(TrackColor::from_engine(track.custom_color()))
    }

    pub fn input(input: Option<RecordingInput>) -> Self {
        Self::Input(TrackInput::from_engine(input))
    }

    pub fn armed(value: bool) -> Self {
        Self::Armed(value)
    }

    pub fn input_monitoring(mode: InputMonitoringMode) -> Self {
        Self::InputMonitoring(TrackInputMonitoring::from_engine(mode).into())
    }

    pub fn mute(value: bool) -> Self {
        Self::Mute(value)
    }

    pub fn solo(value: bool) -> Self {
        Self::Solo(value)
    }

    pub fn selected(value: bool) -> Self {
        Self::Selected(value)
    }

    pub fn volume(db: Db) -> Self {
        Self::Volume(db.get())
    }

    pub fn pan(pan: ReaperPanValue) -> Self {
        Self::Pan(pan.get())
    }
}

impl qualified_occasional_slot_update::Update {
    pub fn play_state(play_state: InternalClipPlayState) -> Self {
        Self::PlayState(SlotPlayState::from_engine(play_state.get()).into())
    }

    pub fn complete_persistent_data(_matrix: &Matrix, slot: &Slot) -> Self {
        let api_slot = slot.save().unwrap_or(playtime_api::persistence::Slot {
            id: slot.id().clone(),
            row: slot.index(),
            clip_old: None,
            clips: None,
        });
        let json = serde_json::to_string(&api_slot).expect("couldn't represent slot as JSON");
        Self::CompletePersistentData(json)
    }
}

impl qualified_occasional_clip_update::Update {
    pub fn complete_persistent_data(_matrix: &Matrix, clip: &Clip) -> ClipEngineResult<Self> {
        let api_clip = clip.save()?;
        let json = serde_json::to_string(&api_clip).expect("couldn't represent clip as JSON");
        Ok(Self::CompletePersistentData(json))
    }
}

impl HistoryState {
    pub fn from_engine(history: &History) -> Self {
        Self {
            undo_label: history
                .next_undo_label()
                .map(|l| l.to_string())
                .unwrap_or_default(),
            redo_label: history
                .next_redo_label()
                .map(|l| l.to_string())
                .unwrap_or_default(),
        }
    }
}

impl SlotAddress {
    pub fn from_engine(address: ClipSlotAddress) -> Self {
        Self {
            column_index: address.column() as _,
            row_index: address.row() as _,
        }
    }

    pub fn to_engine(&self) -> ClipSlotAddress {
        ClipSlotAddress::new(self.column_index as _, self.row_index as _)
    }
}

impl ClipAddress {
    pub fn from_engine(address: base::ClipAddress) -> Self {
        Self {
            slot_address: Some(SlotAddress::from_engine(address.slot_address)),
            clip_index: address.clip_index as _,
        }
    }

    pub fn to_engine(&self) -> Result<base::ClipAddress, &'static str> {
        let addr = base::ClipAddress {
            slot_address: self
                .slot_address
                .as_ref()
                .ok_or("slot address missing")?
                .to_engine(),
            clip_index: self.clip_index as usize,
        };
        Ok(addr)
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

impl ArrangementPlayState {
    pub fn from_engine(play_state: reaper_medium::PlayState) -> Self {
        if play_state.is_recording {
            if play_state.is_paused {
                Self::RecordingPaused
            } else {
                Self::Recording
            }
        } else if play_state.is_playing {
            if play_state.is_paused {
                Self::PlayingPaused
            } else {
                Self::Playing
            }
        } else if play_state.is_paused {
            Self::PlayingPaused
        } else {
            Self::Stopped
        }
    }
}
