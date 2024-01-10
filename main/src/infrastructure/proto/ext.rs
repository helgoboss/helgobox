use helgoboss_midi::Channel;
use reaper_high::{Project, Reaper};
use reaper_medium::{
    Bpm, Db, MidiInputDeviceId, PlayState, ReaperPanValue, ReaperString, RecordingInput, RgbColor,
};
use std::num::NonZeroU32;

use crate::infrastructure::data::{
    ControllerManager, FileBasedControllerPresetManager, FileBasedMainPresetManager,
};
use crate::infrastructure::plugin::InstanceShell;
use crate::infrastructure::proto::track_input::Input;
use crate::infrastructure::proto::{
    clip_content_info, event_reply, generated, occasional_global_update,
    occasional_instance_update, occasional_matrix_update, occasional_track_update,
    qualified_occasional_clip_update, qualified_occasional_column_update,
    qualified_occasional_row_update, qualified_occasional_slot_update, ArrangementPlayState,
    AudioClipContentInfo, AudioInputChannel, AudioInputChannels, ClipAddress, ClipContentInfo,
    ContinuousClipUpdate, ContinuousColumnUpdate, ContinuousMatrixUpdate, ContinuousSlotUpdate,
    GetContinuousColumnUpdatesReply, GetContinuousMatrixUpdatesReply,
    GetContinuousSlotUpdatesReply, GetOccasionalClipUpdatesReply, GetOccasionalColumnUpdatesReply,
    GetOccasionalGlobalUpdatesReply, GetOccasionalInstanceUpdatesReply,
    GetOccasionalMatrixUpdatesReply, GetOccasionalRowUpdatesReply, GetOccasionalSlotUpdatesReply,
    GetOccasionalTrackUpdatesReply, HistoryState, LearnState, MidiClipContentInfo,
    MidiDeviceStatus, MidiInputDevice, MidiInputDevices, MidiOutputDevice, MidiOutputDevices,
    OccasionalGlobalUpdate, OccasionalInstanceUpdate, OccasionalMatrixUpdate,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate,
    QualifiedOccasionalRowUpdate, QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
    SequencerPlayState, SlotAddress, SlotPlayState, TimeSignature, TrackColor, TrackInput,
    TrackInputMonitoring, TrackList, TrackMidiInput,
};
use playtime_clip_engine::base::{
    Clip, ClipSource, ColumnTrackInputMonitoring, History, Matrix, MatrixSequencer, SaveOptions,
    Slot,
};
use playtime_clip_engine::rt::{
    ClipPlayState, ContinuousClipChangeEvent, ContinuousClipChangeEvents, InternalClipPlayState,
};
use playtime_clip_engine::{base, clip_timeline, Timeline};
use realearn_api::runtime::{ControllerPreset, MainPreset};

impl occasional_instance_update::Update {
    pub fn settings(instance_shell: &InstanceShell) -> Self {
        let settings = instance_shell.settings();
        let json =
            serde_json::to_string(&settings).expect("couldn't represent instance settings as JSON");
        Self::Settings(json)
    }
}

impl occasional_global_update::Update {
    pub fn info_event(event: realearn_api::runtime::InfoEvent) -> Self {
        let json =
            serde_json::to_string(&event).expect("couldn't represent main info event as JSON");
        Self::InfoEvent(json)
    }

    pub fn midi_input_devices() -> Self {
        Self::MidiInputDevices(MidiInputDevices::from_engine(
            Reaper::get()
                .midi_input_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn midi_output_devices() -> Self {
        Self::MidiOutputDevices(MidiOutputDevices::from_engine(
            Reaper::get()
                .midi_output_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn audio_input_channels() -> Self {
        Self::AudioInputChannels(AudioInputChannels::from_engine(
            Reaper::get().input_channels(),
        ))
    }

    pub fn controller_presets(manager: &FileBasedControllerPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| ControllerPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json = serde_json::to_string(&api_presets)
            .expect("couldn't represent controller presets as JSON");
        Self::ControllerPresets(json)
    }

    pub fn main_presets(manager: &FileBasedMainPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| MainPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json =
            serde_json::to_string(&api_presets).expect("couldn't represent main presets as JSON");
        Self::MainPresets(json)
    }

    pub fn controller_config(manager: &ControllerManager) -> Self {
        let json = serde_json::to_string(manager.controller_config())
            .expect("couldn't represent controller config as JSON");
        Self::ControllerConfig(json)
    }
}

impl occasional_matrix_update::Update {
    pub fn volume(db: Db) -> Self {
        Self::Volume(db.get())
    }

    pub fn pan(pan: ReaperPanValue) -> Self {
        Self::Pan(pan.get())
    }

    pub fn mute(mute: bool) -> Self {
        Self::Mute(mute)
    }

    pub fn tempo(bpm: Bpm) -> Self {
        Self::Tempo(bpm.get())
    }

    pub fn arrangement_play_state(play_state: PlayState) -> Self {
        Self::ArrangementPlayState(ArrangementPlayState::from_engine(play_state).into())
    }

    pub fn sequencer_play_state(play_state: base::SequencerStatus) -> Self {
        Self::SequencerPlayState(SequencerPlayState::from_engine(play_state).into())
    }

    pub fn sequencer(sequencer: &MatrixSequencer) -> Self {
        let api_sequencer = sequencer.save(SaveOptions {
            include_contents: false,
        });
        let json =
            serde_json::to_string(&api_sequencer).expect("couldn't represent sequencer as JSON");
        Self::Sequencer(json)
    }

    pub fn info_event(event: playtime_api::runtime::InfoEvent) -> Self {
        let json = serde_json::to_string(&event).expect("couldn't represent info event as JSON");
        Self::InfoEvent(json)
    }

    pub fn simple_mappings(matrix: &Matrix) -> Self {
        let container = matrix.get_simple_mappings();
        let json =
            serde_json::to_string(&container).expect("couldn't represent simple mappings as JSON");
        Self::SimpleMappingContainer(json)
    }

    pub fn learn_state(matrix: &Matrix) -> Self {
        let learning_target = matrix.get_currently_learning_target();
        Self::LearnState(LearnState {
            simple_mapping_target: learning_target.map(|s| {
                serde_json::to_string(&s).expect("couldn't represent learning target as JSON")
            }),
        })
    }

    pub fn active_slot(matrix: &Matrix) -> Self {
        let active_slot = matrix.active_slot();
        Self::ActiveSlot(SlotAddress::from_engine(active_slot))
    }

    pub fn complete_persistent_data(matrix: &Matrix) -> Self {
        let matrix_json =
            serde_json::to_string(&matrix.save_internal(SaveOptions::without_contents()))
                .expect("couldn't represent matrix as JSON");
        Self::CompletePersistentData(matrix_json)
    }

    pub fn settings(matrix: &Matrix) -> Self {
        let settings_json = serde_json::to_string(&matrix.all_matrix_settings_combined())
            .expect("couldn't represent matrix settings as JSON");
        Self::Settings(settings_json)
    }

    pub fn history_state(matrix: &Matrix) -> Self {
        Self::HistoryState(HistoryState::from_engine(matrix.history()))
    }

    pub fn click_enabled(matrix: &Matrix) -> Self {
        Self::ClickEnabled(matrix.click_is_enabled())
    }

    pub fn silence_mode(matrix: &Matrix) -> Self {
        Self::SilenceMode(matrix.is_in_silence_mode())
    }

    pub fn time_signature(project: Project) -> Self {
        Self::TimeSignature(TimeSignature::from_engine(project))
    }

    pub fn track_list(project: Project) -> Self {
        Self::TrackList(TrackList::from_engine(project))
    }
}

impl generated::TrackList {
    pub fn from_engine(project: Project) -> Self {
        let mut level = 0i32;
        let tracks = project.tracks().map(|t| {
            let folder_depth_change = t.folder_depth_change();
            let track = generated::TrackInList::from_engine(t, level.unsigned_abs());
            level += folder_depth_change;
            track
        });
        Self {
            tracks: tracks.collect(),
        }
    }
}

impl generated::TrackInList {
    pub fn from_engine(track: reaper_high::Track, level: u32) -> Self {
        Self {
            id: track.guid().to_string_without_braces(),
            name: track.name().unwrap_or_default().into_string(),
            level,
        }
    }
}

impl TimeSignature {
    pub fn to_engine(&self) -> Result<reaper_medium::TimeSignature, &'static str> {
        let sig = reaper_medium::TimeSignature {
            numerator: NonZeroU32::new(self.numerator).ok_or("numerator is zero")?,
            denominator: NonZeroU32::new(self.denominator).ok_or("denominator is zero")?,
        };
        Ok(sig)
    }

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

    pub fn input_monitoring(value: Option<ColumnTrackInputMonitoring>) -> Self {
        let api_value = TrackInputMonitoring::from_engine(value);
        Self::InputMonitoring(api_value.into())
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
        let api_slot =
            slot.save(SaveOptions::without_contents())
                .unwrap_or(playtime_api::persistence::Slot {
                    id: slot.id().clone(),
                    row: slot.index(),
                    clip_old: None,
                    clips: None,
                });
        let json = serde_json::to_string(&api_slot).expect("couldn't represent slot as JSON");
        Self::CompletePersistentData(json)
    }
}

impl qualified_occasional_column_update::Update {
    pub fn settings(matrix: &Matrix, column_index: usize) -> anyhow::Result<Self> {
        let column = matrix.get_column(column_index)?;
        let json = serde_json::to_string(&column.all_column_settings_combined())
            .expect("couldn't represent slot as JSON");
        Ok(Self::Settings(json))
    }
}

impl qualified_occasional_row_update::Update {
    pub fn data(matrix: &Matrix, row_index: usize) -> anyhow::Result<Self> {
        let row = matrix.get_row(row_index)?;
        let json = serde_json::to_string(&row.save()).expect("couldn't represent row as JSON");
        Ok(Self::Data(json))
    }
}

impl qualified_occasional_clip_update::Update {
    pub fn complete_persistent_data(clip: &Clip) -> anyhow::Result<Self> {
        let api_clip = clip.save(SaveOptions::without_contents())?;
        let json = serde_json::to_string(&api_clip).expect("couldn't represent clip as JSON");
        Ok(Self::CompletePersistentData(json))
    }

    pub fn content_info(clip: &Clip) -> Self {
        Self::ContentInfo(ClipContentInfo {
            info: Some(clip_content_info::Info::from_engine(clip)),
        })
    }
}

impl clip_content_info::Info {
    pub fn from_engine(clip: &Clip) -> Self {
        match clip.source() {
            ClipSource::File(_) => Self::Audio(AudioClipContentInfo {}),
            ClipSource::MidiSequence(s) => Self::Midi(MidiClipContentInfo {
                quantized: s.is_quantized(),
            }),
        }
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
    pub fn from_engine(address: playtime_api::persistence::SlotAddress) -> Self {
        Self {
            column_index: address.column() as _,
            row_index: address.row() as _,
        }
    }

    pub fn to_engine(&self) -> playtime_api::persistence::SlotAddress {
        playtime_api::persistence::SlotAddress::new(self.column_index as _, self.row_index as _)
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
            Ignited => Self::Ignited,
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

    pub fn to_engine(&self) -> Option<RgbColor> {
        let c = self.color?;
        let dest = RgbColor {
            r: ((c >> 16) & 0xFF) as u8,
            g: ((c >> 8) & 0xFF) as u8,
            b: (c & 0xFF) as u8,
        };
        Some(dest)
    }
}

impl TrackInput {
    pub fn from_engine(input: Option<RecordingInput>) -> Self {
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

    pub fn to_engine(&self) -> Option<RecordingInput> {
        let input = match self.input.as_ref()? {
            Input::Mono(ch) => RecordingInput::Mono(*ch),
            Input::Stereo(ch) => RecordingInput::Stereo(*ch),
            Input::Midi(input) => RecordingInput::Midi {
                device_id: input.device.map(|id| MidiInputDeviceId::new(id as _)),
                channel: input.channel.map(|id| Channel::new(id as _)),
            },
        };
        Some(input)
    }
}

impl TrackInputMonitoring {
    pub fn from_engine(value: Option<ColumnTrackInputMonitoring>) -> Self {
        use ColumnTrackInputMonitoring::*;
        match value {
            None => Self::Unknown,
            Some(Off) => Self::Off,
            Some(Auto) => Self::Auto,
            Some(On) => Self::On,
        }
    }

    pub fn to_engine(&self) -> Option<ColumnTrackInputMonitoring> {
        use ColumnTrackInputMonitoring as T;
        use TrackInputMonitoring::*;
        match self {
            Unknown => None,
            Off => Some(T::Off),
            Auto => Some(T::Auto),
            On => Some(T::On),
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

impl SequencerPlayState {
    pub fn from_engine(play_state: base::SequencerStatus) -> Self {
        use base::SequencerStatus as S;
        use SequencerPlayState as T;
        match play_state {
            S::Stopped => T::Stopped,
            S::Playing => T::Playing,
            S::Recording => T::Recording,
        }
    }
}

impl ContinuousSlotUpdate {
    pub fn from_engine(clip_events: &ContinuousClipChangeEvents) -> Self {
        Self {
            clip_update: clip_events
                .iter()
                .map(ContinuousClipUpdate::from_engine)
                .collect(),
        }
    }
}

impl ContinuousClipUpdate {
    pub fn from_engine(event: &ContinuousClipChangeEvent) -> Self {
        Self {
            proportional_position: event.proportional.get(),
            position_in_seconds: event.seconds.get(),
            source_position_in_frames: event.source_pos_in_frames,
            peak: event.peak.get(),
        }
    }
}

impl MidiInputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiInputDevice>) -> Self {
        Self {
            devices: devs.map(MidiInputDevice::from_engine).collect(),
        }
    }
}

impl MidiInputDevice {
    pub fn from_engine(dev: reaper_high::MidiInputDevice) -> Self {
        MidiInputDevice {
            id: dev.id().get() as _,
            name: dev.name().into_string(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiOutputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiOutputDevice>) -> Self {
        Self {
            devices: devs.map(MidiOutputDevice::from_engine).collect(),
        }
    }
}

impl MidiOutputDevice {
    pub fn from_engine(dev: reaper_high::MidiOutputDevice) -> Self {
        MidiOutputDevice {
            id: dev.id().get() as _,
            name: dev.name().into_string(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiDeviceStatus {
    pub fn from_engine(open: bool, connected: bool) -> Self {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}

impl AudioInputChannels {
    pub fn from_engine(channels: impl Iterator<Item = ReaperString>) -> Self {
        Self {
            channels: channels
                .enumerate()
                .map(|(i, name)| AudioInputChannel {
                    index: i as u32,
                    name: name.into_string(),
                })
                .collect(),
        }
    }
}

impl From<Vec<OccasionalGlobalUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalGlobalUpdate>) -> Self {
        event_reply::Value::OccasionalGlobalUpdatesReply(GetOccasionalGlobalUpdatesReply {
            global_updates: value,
        })
    }
}

impl From<Vec<OccasionalInstanceUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalInstanceUpdate>) -> Self {
        event_reply::Value::OccasionalInstanceUpdatesReply(GetOccasionalInstanceUpdatesReply {
            instance_updates: value,
        })
    }
}

impl From<Vec<OccasionalMatrixUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalMatrixUpdate>) -> Self {
        event_reply::Value::OccasionalMatrixUpdatesReply(GetOccasionalMatrixUpdatesReply {
            matrix_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalTrackUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalTrackUpdate>) -> Self {
        event_reply::Value::OccasionalTrackUpdatesReply(GetOccasionalTrackUpdatesReply {
            track_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalColumnUpdate>) -> Self {
        event_reply::Value::OccasionalColumnUpdatesReply(GetOccasionalColumnUpdatesReply {
            column_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalRowUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalRowUpdate>) -> Self {
        event_reply::Value::OccasionalRowUpdatesReply(GetOccasionalRowUpdatesReply {
            row_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalSlotUpdate>) -> Self {
        event_reply::Value::OccasionalSlotUpdatesReply(GetOccasionalSlotUpdatesReply {
            slot_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalClipUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalClipUpdate>) -> Self {
        event_reply::Value::OccasionalClipUpdatesReply(GetOccasionalClipUpdatesReply {
            clip_updates: value,
        })
    }
}

impl From<ContinuousMatrixUpdate> for event_reply::Value {
    fn from(value: ContinuousMatrixUpdate) -> Self {
        event_reply::Value::ContinuousMatrixUpdatesReply(GetContinuousMatrixUpdatesReply {
            matrix_update: Some(value),
        })
    }
}

impl From<Vec<ContinuousColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<ContinuousColumnUpdate>) -> Self {
        event_reply::Value::ContinuousColumnUpdatesReply(GetContinuousColumnUpdatesReply {
            column_updates: value,
        })
    }
}

impl From<Vec<QualifiedContinuousSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedContinuousSlotUpdate>) -> Self {
        event_reply::Value::ContinuousSlotUpdatesReply(GetContinuousSlotUpdatesReply {
            slot_updates: value,
        })
    }
}
