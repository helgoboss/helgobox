use std::num::NonZeroU32;

use helgoboss_license_api::persistence::LicenseData;
use helgoboss_license_api::runtime::License;
use helgoboss_midi::Channel;
use reaper_high::{Project, Reaper};
use reaper_medium::{Db, MidiInputDeviceId, ReaperPanValue, RecordingInput};

use playtime_api::runtime::ControlUnitConfig;
use playtime_clip_engine::base::{
    Clip, ClipEmployment, ClipSource, ColumnTrackInputMonitoring, History, Matrix, MatrixSequencer,
    SaveOptions, SequencerStatus, Slot,
};
use playtime_clip_engine::rt::{
    ClipPlayState, ContinuousClipChangeEvent, ContinuousClipChangeEvents,
};
use playtime_clip_engine::{
    base, clip_timeline, PlaytimeEngine, SteadyProjectTimelineHandle, Timeline,
};

use crate::infrastructure::proto::track_input::Input;
use crate::infrastructure::proto::{
    clip_content_info, generated, occasional_matrix_update, occasional_playtime_engine_update,
    occasional_track_update, qualified_occasional_clip_update, qualified_occasional_column_update,
    qualified_occasional_row_update, qualified_occasional_slot_update, AudioClipContentInfo,
    CellAddress, ClipAddress, ClipContentInfo, ColumnKind, ContinuousClipUpdate,
    ContinuousSlotUpdate, Fx, FxChain, HistoryState, LearnState, LicenseState, MidiClipContentInfo,
    PlaytimeEngineStats, RgbColor, SequencerPlayState, SlotAddress, SlotPlayState, TimeSignature,
    TrackInput, TrackInputMonitoring, TrackList, TrackMidiInput,
};

impl occasional_playtime_engine_update::Update {
    pub fn engine_settings() -> Self {
        let settings = PlaytimeEngine::get().main().settings();
        let json = serde_json::to_string(&settings).expect("couldn't serialize playtime settings");
        Self::EngineSettings(json)
    }

    pub fn engine_stats() -> Self {
        let project = Reaper::get().current_project();
        SteadyProjectTimelineHandle::new(project).with_timeline(|timeline| {
            let pre_buffer_stat = timeline.last_pre_buffered_blocks_of_playing_clips_stat();
            let engine_stats = PlaytimeEngine::get().stats();
            let stats = PlaytimeEngineStats {
                min_buffered_blocks: pre_buffer_stat.min() as _,
                avg_buffered_blocks: pre_buffer_stat.avg() as _,
                max_buffered_blocks: pre_buffer_stat.max() as _,
                future_size_in_blocks: timeline.tempo_buffer_future_size() as _,
                num_pre_buffer_fallbacks: engine_stats.num_pre_buffer_fallbacks,
                num_pre_buffer_misses: engine_stats.num_pre_buffer_misses,
            };
            Self::EngineStats(stats)
        })
    }

    pub fn playtime_license_state() -> Self {
        let value = {
            #[cfg(feature = "playtime")]
            {
                let clip_engine = playtime_clip_engine::PlaytimeEngine::get().main();
                if clip_engine.has_valid_license() {
                    clip_engine.license()
                } else {
                    None
                }
            }
            #[cfg(not(feature = "playtime"))]
            {
                None
            }
        };
        let json = value.map(|license: License| {
            let license_data = LicenseData::from(license);
            serde_json::to_string(&license_data.payload)
                .expect("couldn't represent license payload as JSON")
        });
        Self::LicenseState(LicenseState {
            license_payload: json,
        })
    }
}

impl occasional_matrix_update::Update {
    pub fn master_volume(db: Db) -> Self {
        Self::MasterVolume(db.get())
    }

    pub fn click_volume(matrix: &Matrix) -> Self {
        Self::ClickVolume(matrix.click_volume().get())
    }

    pub fn tempo_tap_volume(matrix: &Matrix) -> Self {
        Self::TempoTapVolume(matrix.tempo_tap_volume().get())
    }

    pub fn pan(pan: ReaperPanValue) -> Self {
        Self::Pan(pan.get())
    }

    pub fn mute(mute: bool) -> Self {
        Self::Mute(mute)
    }

    pub fn tempo(matrix: &Matrix) -> Self {
        Self::Tempo(matrix.project_tempo().get())
    }

    pub fn play_rate(matrix: &Matrix) -> Self {
        Self::PlayRate(matrix.play_rate().get())
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

    pub fn active_cell(matrix: &Matrix) -> Self {
        let active_cell = matrix.active_cell();
        Self::ActiveCell(CellAddress::from_engine(active_cell))
    }

    pub fn control_unit_config(matrix: &Matrix) -> Self {
        let config = ControlUnitConfig {
            control_units: matrix.get_control_units(),
        };
        let json =
            serde_json::to_string(&config).expect("couldn't represent control unit config as JSON");
        Self::ControlUnitConfig(json)
    }

    pub fn complete_persistent_data(matrix: &Matrix) -> Self {
        let matrix_json = serde_json::to_string(&matrix.save_without_contents())
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

    pub fn has_unloaded_content(matrix: &Matrix) -> Self {
        Self::HasUnloadedContent(matrix.has_unloaded_content())
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

impl ColumnKind {
    pub fn to_engine(self) -> playtime_clip_engine::base::ColumnKind {
        match self {
            ColumnKind::Audio => base::ColumnKind::Audio,
            ColumnKind::Midi => base::ColumnKind::Midi,
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
        let timeline = clip_timeline(project, true);
        let time_signature = timeline.time_signature_at(timeline.cursor_pos().get());
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
        Self::Color(RgbColor::from_engine(track.custom_color()))
    }

    pub fn input(track: &reaper_high::Track) -> Self {
        let engine_track_input = base::TrackInput::from_track(track);
        Self::Input(TrackInput::from_engine(engine_track_input))
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

    pub fn normal_fx_chain(track: &reaper_high::Track) -> Self {
        Self::NormalFxChain(FxChain::from_engine(&track.normal_fx_chain()))
    }
}

impl FxChain {
    pub fn from_engine(fx_chain: &reaper_high::FxChain) -> Self {
        Self {
            fxs: fx_chain
                .index_based_fxs()
                .map(|fx| Fx::from_engine(&fx))
                .collect(),
        }
    }
}

impl Fx {
    pub fn from_engine(fx: &reaper_high::Fx) -> Self {
        Self {
            name: fx.name().to_string(),
            // No need to fall back to chunk-based FX info because Playtime doesn't support old REAPER versions anyway.
            instrument: fx
                .info()
                .map(|info| info.sub_type_expression.ends_with('i'))
                .unwrap_or(false),
        }
    }
}

impl qualified_occasional_slot_update::Update {
    pub fn play_state(play_state: ClipPlayState) -> Self {
        Self::PlayState(SlotPlayState::from_engine(play_state).into())
    }

    pub fn complete_persistent_data(matrix: &Matrix, slot: &Slot) -> Self {
        let base_dir = matrix.base_dir();
        let api_slot = slot
            .save(base_dir.as_deref(), SaveOptions::without_contents())
            .unwrap_or(playtime_api::persistence::Slot {
                id: slot.id().clone(),
                row: slot.index(),
                clip_old: None,
                clips: None,
                ignited: false,
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
    pub fn complete_persistent_data(matrix: &Matrix, clip: &Clip) -> anyhow::Result<Self> {
        let base_dir = matrix.base_dir();
        let api_clip = clip.save(base_dir.as_deref(), SaveOptions::without_contents())?;
        let json = serde_json::to_string(&api_clip).expect("couldn't represent clip as JSON");
        Ok(Self::CompletePersistentData(json))
    }

    pub fn content_info(clip_employment: &ClipEmployment) -> Self {
        Self::ContentInfo(ClipContentInfo {
            info: Some(clip_content_info::Info::from_engine(clip_employment)),
        })
    }
}

impl clip_content_info::Info {
    pub fn from_engine(clip_employment: &ClipEmployment) -> Self {
        match clip_employment.clip.source() {
            ClipSource::File(_) => Self::Audio(AudioClipContentInfo {
                online: clip_employment.online_data.is_some(),
            }),
            ClipSource::Midi(s) => Self::Midi(MidiClipContentInfo {
                quantized: s.sequence().is_quantized(),
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
            ScheduledForPlayRestart => Self::ScheduledForPlayRestart,
            ScheduledForPlayStop => Self::ScheduledForPlayStop,
            ScheduledForRecordingStart => Self::ScheduledForRecordingStart,
            Recording => Self::Recording,
            ScheduledForRecordingStop => Self::ScheduledForRecordingStop,
        }
    }
}

impl TrackInput {
    pub fn from_engine(input: base::TrackInput) -> Self {
        use RecordingInput::*;
        let input = match input.rec_input {
            Some(Mono(ch)) => Some(Input::Mono(ch)),
            Some(Stereo(ch)) => Some(Input::Stereo(ch)),
            Some(MonoReaRoute(ch)) => Some(Input::LoopbackMono(ch - 256)),
            Some(StereoReaRoute(ch)) => Some(Input::LoopbackStereo(ch - 256)),
            Some(Midi { device_id, channel }) => {
                let midi_input = TrackMidiInput {
                    device: device_id.map(|id| id.get() as _),
                    channel: channel.map(|ch| ch.get() as _),
                    destination_channel: input.map_to_midi_channel.map(|ch| ch.get() as _),
                };
                Some(Input::Midi(midi_input))
            }
            _ => None,
        };
        Self { input }
    }

    pub fn to_engine(&self) -> base::TrackInput {
        let (rec_input, map_to_midi_channel) = match self.input.as_ref() {
            None => (None, None),
            Some(Input::Mono(ch)) => (Some(RecordingInput::Mono(*ch)), None),
            Some(Input::Stereo(ch)) => (Some(RecordingInput::Stereo(*ch)), None),
            Some(Input::LoopbackMono(ch)) => (Some(RecordingInput::MonoReaRoute(256 + ch)), None),
            Some(Input::LoopbackStereo(ch)) => {
                (Some(RecordingInput::StereoReaRoute(256 + ch)), None)
            }
            Some(Input::Midi(input)) => {
                let midi_input = RecordingInput::Midi {
                    device_id: input.device.map(|id| MidiInputDeviceId::new(id as _)),
                    channel: input.channel.map(|id| Channel::new(id as _)),
                };
                let map_to_midi_channel = input
                    .destination_channel
                    .and_then(|ch| Channel::try_from(ch).ok());
                (Some(midi_input), map_to_midi_channel)
            }
        };
        base::TrackInput {
            rec_input,
            map_to_midi_channel,
        }
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

    pub fn to_engine(self) -> Option<ColumnTrackInputMonitoring> {
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

impl SequencerPlayState {
    pub fn from_engine(play_state: base::SequencerStatus) -> Self {
        use SequencerStatus::*;
        match play_state {
            Stopped => Self::Stopped,
            Playing => Self::Playing,
            Recording => Self::Recording,
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
            hypothetical_source_position_in_frames: event.hypothetical_source_pos_in_frames,
            peak: event.peak.get(),
        }
    }
}
