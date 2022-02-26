use crate::main::{ClipContent, ClipData, Column};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, keep_stretching, RecorderEquipment, StretchWorkerRequest,
};
use crate::rt::{
    ClipChangedEvent, ClipInfo, ClipPlayState, ColumnPlayClipArgs, ColumnStopClipArgs,
    RecordBehavior, RecordTiming, RtMatrixCommandSender, SharedColumnSource, WeakColumnSource,
    FAKE_ROW_INDEX,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, ClipRecordStartTiming, ClipRecordStopTiming,
    ClipRecordTimeBase, ClipSettingOverrideAfterRecording, ColumnClipPlaySettings,
    MatrixClipPlayAudioSettings, MatrixClipPlaySettings, MatrixClipRecordAudioSettings,
    MatrixClipRecordMidiSettings, MatrixClipRecordSettings, MidiClipRecordMode, RecordLength,
    TempoRange, TrackId, VirtualResampleMode,
};
use reaper_high::{Guid, Item, OrCurrentProject, Project, Track};
use reaper_medium::{Bpm, PositionInSeconds, ReaperVolumeValue};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug)]
pub struct Matrix<H> {
    settings: MatrixSettings,
    rt_settings: rt::MatrixSettings,
    handler: H,
    #[allow(dead_code)]
    stretch_worker_sender: Sender<StretchWorkerRequest>,
    recorder_equipment: RecorderEquipment,
    columns: Vec<Column>,
    containing_track: Option<Track>,
    command_receiver: Receiver<MatrixCommand>,
    rt_command_sender: Sender<rt::MatrixCommand>,
    // We use this just for RAII (joining worker threads when dropped)
    _worker_pool: WorkerPool,
}

#[derive(Debug, Default)]
pub struct MatrixSettings {
    pub common_tempo_range: TempoRange,
    pub audio_resample_mode: VirtualResampleMode,
    pub audio_time_stretch_mode: AudioTimeStretchMode,
    pub audio_cache_behavior: AudioCacheBehavior,
}

#[derive(Debug)]
pub enum MatrixCommand {
    ThrowAway(WeakColumnSource),
}

pub trait MainMatrixCommandSender {
    fn throw_away(&self, source: WeakColumnSource);
    fn send_command(&self, command: MatrixCommand);
}

impl MainMatrixCommandSender for Sender<MatrixCommand> {
    fn throw_away(&self, source: WeakColumnSource) {
        self.send_command(MatrixCommand::ThrowAway(source));
    }

    fn send_command(&self, command: MatrixCommand) {
        self.try_send(command).unwrap();
    }
}

#[derive(Debug, Default)]
struct WorkerPool {
    workers: Vec<Worker>,
}

#[derive(Debug)]
struct Worker {
    join_handle: Option<JoinHandle<()>>,
}

impl WorkerPool {
    pub fn add_worker(&mut self, name: &str, f: impl FnOnce() + Send + 'static) {
        let join_handle = thread::Builder::new()
            .name(String::from(name))
            .spawn(f)
            .unwrap();
        let worker = Worker {
            join_handle: Some(join_handle),
        };
        self.workers.push(worker);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(join_handle) = self.join_handle.take() {
            let name = join_handle.thread().name().unwrap();
            debug!("Shutting down clip matrix worker \"{}\"...", name);
            join_handle.join().unwrap();
        }
    }
}

impl<H: ClipMatrixHandler> Matrix<H> {
    pub fn new(handler: H, containing_track: Option<Track>) -> (Self, rt::Matrix) {
        let (stretch_worker_sender, stretch_worker_receiver) = crossbeam_channel::bounded(500);
        let (recorder_request_sender, recorder_request_receiver) = crossbeam_channel::bounded(500);
        let (cache_request_sender, cache_request_receiver) = crossbeam_channel::bounded(500);
        let (pre_buffer_request_sender, pre_buffer_request_receiver) =
            crossbeam_channel::bounded(500);
        let (rt_command_sender, rt_command_receiver) = crossbeam_channel::bounded(500);
        let (main_command_sender, main_command_receiver) = crossbeam_channel::bounded(500);
        let mut worker_pool = WorkerPool::default();
        worker_pool.add_worker("Playtime stretch worker", move || {
            keep_stretching(stretch_worker_receiver);
        });
        worker_pool.add_worker("Playtime recording worker", move || {
            keep_processing_recorder_requests(recorder_request_receiver);
        });
        worker_pool.add_worker("Playtime cache worker", move || {
            keep_processing_cache_requests(cache_request_receiver);
        });
        worker_pool.add_worker("Playtime pre-buffer worker", move || {
            keep_processing_pre_buffer_requests(pre_buffer_request_receiver);
        });
        let project = containing_track.as_ref().map(|t| t.project());
        let matrix = Self {
            settings: Default::default(),
            rt_settings: Default::default(),
            handler,
            stretch_worker_sender,
            recorder_equipment: RecorderEquipment {
                recorder_request_sender,
                cache_request_sender,
                pre_buffer_request_sender,
            },
            columns: vec![],
            containing_track,
            command_receiver: main_command_receiver,
            rt_command_sender,
            _worker_pool: worker_pool,
        };
        let rt_matrix = rt::Matrix::new(rt_command_receiver, main_command_sender, project);
        (matrix, rt_matrix)
    }

    pub fn load(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
        self.clear_columns();
        let permanent_project = self.permanent_project();
        // Settings
        self.settings.common_tempo_range = api_matrix.common_tempo_range;
        self.settings.audio_resample_mode =
            api_matrix.clip_play_settings.audio_settings.resample_mode;
        self.settings.audio_time_stretch_mode = api_matrix
            .clip_play_settings
            .audio_settings
            .time_stretch_mode;
        self.settings.audio_cache_behavior =
            api_matrix.clip_play_settings.audio_settings.cache_behavior;
        self.rt_settings.clip_play_start_timing = api_matrix.clip_play_settings.start_timing;
        self.rt_settings.clip_play_stop_timing = api_matrix.clip_play_settings.stop_timing;
        self.rt_command_sender
            .update_settings(self.rt_settings.clone());
        // Columns
        for (i, api_column) in api_matrix
            .columns
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let mut column = Column::new(permanent_project);
            column.load(
                api_column,
                permanent_project,
                &self.recorder_equipment,
                &self.settings,
            )?;
            self.rt_command_sender.insert_column(i, column.source());
            self.columns.push(column);
        }
        self.handler.notify_slot_contents_changed();
        Ok(())
    }

    pub fn save(&self) -> api::Matrix {
        api::Matrix {
            columns: Some(self.columns.iter().map(|column| column.save()).collect()),
            rows: None,
            clip_play_settings: MatrixClipPlaySettings {
                start_timing: self.rt_settings.clip_play_start_timing,
                stop_timing: self.rt_settings.clip_play_stop_timing,
                audio_settings: MatrixClipPlayAudioSettings {
                    resample_mode: self.settings.audio_resample_mode.clone(),
                    time_stretch_mode: self.settings.audio_time_stretch_mode.clone(),
                    cache_behavior: self.settings.audio_cache_behavior.clone(),
                },
            },
            clip_record_settings: MatrixClipRecordSettings {
                start_timing: ClipRecordStartTiming::LikeClipPlayStartTiming,
                stop_timing: ClipRecordStopTiming::LikeClipRecordStartTiming,
                duration: RecordLength::OpenEnd,
                play_start_timing: ClipSettingOverrideAfterRecording::Inherit,
                play_stop_timing: ClipSettingOverrideAfterRecording::Inherit,
                time_base: ClipRecordTimeBase::Time,
                play_after: false,
                lead_tempo: false,
                midi_settings: MatrixClipRecordMidiSettings {
                    record_mode: MidiClipRecordMode::Normal,
                    detect_downbeat: false,
                    detect_input: false,
                    auto_quantize: false,
                },
                audio_settings: MatrixClipRecordAudioSettings {
                    detect_downbeat: false,
                    detect_input: false,
                },
            },
            common_tempo_range: self.settings.common_tempo_range,
        }
    }

    fn permanent_project(&self) -> Option<Project> {
        self.containing_track.as_ref().map(|t| t.project())
    }

    pub fn clear_columns(&mut self) {
        // TODO-medium How about suspension?
        self.columns.clear();
        self.rt_command_sender.clear_columns();
    }

    /// This is for loading slots the legacy way.
    pub fn load_legacy(&mut self, descriptors: Vec<LegacySlotDescriptor>) -> ClipEngineResult<()> {
        let api_matrix = api::Matrix {
            columns: {
                let api_columns: ClipEngineResult<Vec<_>> = descriptors
                    .into_iter()
                    .map(|desc| {
                        let api_column = api::Column {
                            clip_play_settings: ColumnClipPlaySettings {
                                track: desc
                                    .output
                                    .resolve_track(self.containing_track.clone())?
                                    .map(|t| TrackId(t.guid().to_string_without_braces())),
                                ..Default::default()
                            },
                            clip_record_settings: Default::default(),
                            slots: {
                                let api_clip = api::Clip {
                                    source: match desc.clip.content {
                                        ClipContent::File { file } => {
                                            api::Source::File(api::FileSource { path: file })
                                        }
                                        ClipContent::MidiChunk { chunk } => {
                                            api::Source::MidiChunk(api::MidiChunkSource { chunk })
                                        }
                                    },
                                    time_base: api::ClipTimeBase::Time,
                                    start_timing: None,
                                    stop_timing: None,
                                    looped: desc.clip.repeat,
                                    volume: api::Db::new(0.0).unwrap(),
                                    color: api::ClipColor::PlayTrackColor,
                                    section: api::Section {
                                        start_pos: api::PositiveSecond::new(0.0).unwrap(),
                                        length: None,
                                    },
                                    audio_settings: Default::default(),
                                    midi_settings: Default::default(),
                                };
                                let api_slot = api::Slot {
                                    row: 0,
                                    clip: Some(api_clip),
                                };
                                Some(vec![api_slot])
                            },
                        };
                        Ok(api_column)
                    })
                    .collect();
                Some(api_columns?)
            },
            ..Default::default()
        };
        self.load(api_matrix)
    }

    pub fn filled_slot_descriptors_legacy(&self) -> Vec<QualifiedSlotDescriptor> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(i, column)| {
                Some(QualifiedSlotDescriptor {
                    index: i,
                    descriptor: column.clip_data(0)?,
                })
            })
            .collect()
    }

    pub fn play_clip(&mut self, column_index: usize) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column_mut(&mut self.columns, column_index)?;
        let args = ColumnPlayClipArgs {
            slot_index: FAKE_ROW_INDEX,
            parent_start_timing: self.rt_settings.clip_play_start_timing,
            timeline,
            ref_pos: None,
        };
        column.play_clip(args);
        Ok(())
    }

    pub fn stop_clip(&mut self, column_index: usize) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column_mut(&mut self.columns, column_index)?;
        let args = ColumnStopClipArgs {
            slot_index: FAKE_ROW_INDEX,
            parent_start_timing: self.rt_settings.clip_play_start_timing,
            parent_stop_timing: self.rt_settings.clip_play_stop_timing,
            timeline,
            ref_pos: None,
        };
        column.stop_clip(args);
        Ok(())
    }

    fn timeline(&self) -> HybridTimeline {
        let project = self.permanent_project().or_current_project();
        clip_timeline(Some(project), false)
    }

    fn process_commands(&mut self) {
        while let Ok(task) = self.command_receiver.try_recv() {
            match task {
                MatrixCommand::ThrowAway(_) => {}
            }
        }
    }

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<(ClipLocation, ClipChangedEvent)> {
        self.process_commands();
        self.columns
            .iter_mut()
            .enumerate()
            .flat_map(|(i, column)| {
                column
                    .poll(timeline_tempo)
                    .into_iter()
                    .map(move |(row, event)| (ClipLocation::new(i, row), event))
            })
            .collect()
    }

    pub fn toggle_repeat_legacy(&mut self, slot_index: usize) -> ClipEngineResult<()> {
        let event = get_column_mut(&mut self.columns, slot_index)?.toggle_clip_repeated(0)?;
        self.handler.notify_clip_changed(slot_index, event);
        Ok(())
    }

    pub fn clip_position_in_seconds(
        &self,
        slot_index: usize,
        timeline_tempo: Bpm,
    ) -> Option<PositionInSeconds> {
        get_column(&self.columns, slot_index)
            .ok()?
            .clip_position_in_seconds(0, timeline_tempo)
    }

    pub fn clip_play_state(&self, slot_index: usize) -> Option<ClipPlayState> {
        get_column(&self.columns, slot_index)
            .ok()?
            .clip_play_state(0)
    }

    pub fn clip_repeated(&self, slot_index: usize) -> Option<bool> {
        get_column(&self.columns, slot_index).ok()?.clip_repeated(0)
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn clip_volume(&self, slot_index: usize) -> Option<ReaperVolumeValue> {
        get_column(&self.columns, slot_index).ok()?.clip_volume(0)
    }

    pub fn clip_data(&self, slot_index: usize) -> Option<ClipData> {
        get_column(&self.columns, slot_index).ok()?.clip_data(0)
    }

    pub fn clip_info(&self, slot_index: usize) -> Option<ClipInfo> {
        get_column(&self.columns, slot_index).ok()?.clip_info(0)
    }

    pub fn record_clip_legacy(
        &mut self,
        slot_index: usize,
        args: RecordArgs,
    ) -> ClipEngineResult<()> {
        let behavior = match args.kind {
            RecordKind::Normal {
                play_after,
                timing,
                detect_downbeat,
            } => RecordBehavior::Normal {
                play_after,
                timing: match timing {
                    ClipRecordTiming::StartImmediatelyStopOnDemand => RecordTiming::Unsynced,
                    ClipRecordTiming::StartOnBarStopOnDemand { start_bar } => {
                        RecordTiming::Synced {
                            start_bar,
                            end_bar: None,
                        }
                    }
                    ClipRecordTiming::StartOnBarStopOnBar {
                        start_bar,
                        bar_count,
                    } => RecordTiming::Synced {
                        start_bar,
                        end_bar: Some(start_bar + bar_count as i32),
                    },
                },
                detect_downbeat,
            },
            RecordKind::MidiOverdub => RecordBehavior::MidiOverdub,
        };
        let task = get_column_mut(&mut self.columns, slot_index)?.record_clip(
            0,
            behavior,
            self.recorder_equipment.clone(),
        )?;
        self.handler.request_recording_input(task);
        Ok(())
    }

    pub fn pause_clip_legacy(&mut self, slot_index: usize) -> ClipEngineResult<()> {
        get_column_mut(&mut self.columns, slot_index)?.pause_clip(0);
        Ok(())
    }

    pub fn seek_clip_legacy(
        &mut self,
        slot_index: usize,
        position: UnitValue,
    ) -> ClipEngineResult<()> {
        get_column_mut(&mut self.columns, slot_index)?.seek_clip(0, position);
        Ok(())
    }

    pub fn set_clip_volume_legacy(
        &mut self,
        slot_index: usize,
        volume: ReaperVolumeValue,
    ) -> ClipEngineResult<()> {
        get_column_mut(&mut self.columns, slot_index)?.set_clip_volume(0, volume);
        Ok(())
    }

    pub fn proportional_clip_position_legacy(&self, slot_index: usize) -> Option<UnitValue> {
        get_column(&self.columns, slot_index)
            .ok()?
            .proportional_clip_position(0)
    }

    pub fn fill_slot_with_item_source(
        &mut self,
        _slot_index: usize,
        item: Item,
    ) -> Result<(), Box<dyn Error>> {
        // let slot = get_slot_mut(&mut self.clip_slots, slot_index)?;
        // let content = ClipContent::from_item(item, false)?;
        // slot.fill_by_user(content, item.project(), &self.stretch_worker_sender)?;
        // self.handler.notify_slot_contents_changed();
        let content = ClipContent::from_item(item, false)?;
        dbg!(content);
        Ok(())
    }
}

fn get_column(columns: &[Column], index: usize) -> ClipEngineResult<&Column> {
    columns.get(index).ok_or(NO_SUCH_COLUMN)
}

fn get_column_mut(columns: &mut [Column], index: usize) -> ClipEngineResult<&mut Column> {
    columns.get_mut(index).ok_or(NO_SUCH_COLUMN)
}

const NO_SUCH_COLUMN: &str = "no such column";

pub struct LegacySlotDescriptor {
    pub output: LegacyClipOutput,
    pub index: usize,
    pub clip: ClipData,
}

pub enum LegacyClipOutput {
    MasterTrack,
    ThisTrack,
    TrackById(Guid),
    TrackByIndex(u32),
    TrackByName(String),
    HardwareOutput,
}

impl LegacyClipOutput {
    fn resolve_track(&self, containing_track: Option<Track>) -> ClipEngineResult<Option<Track>> {
        use LegacyClipOutput::*;
        let containing_track = containing_track.ok_or(
            "track-based columns are not supported when clip engine runs in monitoring FX chain",
        );
        let track = match self {
            MasterTrack => Some(containing_track?.project().master_track()),
            ThisTrack => Some(containing_track?),
            TrackById(id) => {
                let track = containing_track?.project().track_by_guid(id);
                if track.is_available() {
                    Some(track)
                } else {
                    None
                }
            }
            TrackByIndex(index) => containing_track?.project().track_by_index(*index),
            TrackByName(name) => containing_track?
                .project()
                .tracks()
                .find(|t| t.name().map(|n| n.to_str() == name).unwrap_or(false)),
            HardwareOutput => None,
        };
        Ok(track)
    }
}

#[derive(Debug)]
pub struct ClipRecordTask {
    pub column_source: SharedColumnSource,
    pub slot_index: usize,
}

pub trait ClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask);
    fn notify_slot_contents_changed(&mut self);
    fn notify_clip_changed(&self, slot_index: usize, event: ClipChangedEvent);
}

#[derive(Copy, Clone, Debug)]
pub enum ClipRecordTiming {
    StartImmediatelyStopOnDemand,
    StartOnBarStopOnDemand { start_bar: i32 },
    StartOnBarStopOnBar { start_bar: i32, bar_count: u32 },
}

// TODO-medium Evolved into a perfect duplicate of ClipStopTime
/// Defines how to stop the clip.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SlotStopBehavior {
    Immediately,
    EndOfClip,
}
/// Contains instructions how to play a clip.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {
    /// Syncs with timeline.
    pub next_bar: bool,
    pub buffered: bool,
}

#[derive(Copy, Clone)]
pub struct RecordArgs {
    pub kind: RecordKind,
}

#[derive(Copy, Clone)]
pub enum RecordKind {
    Normal {
        play_after: bool,
        timing: ClipRecordTiming,
        detect_downbeat: bool,
    },
    MidiOverdub,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QualifiedSlotDescriptor {
    #[serde(rename = "index")]
    pub index: usize,
    #[serde(flatten)]
    pub descriptor: ClipData,
}

pub struct ClipLocation {
    pub column: usize,
    pub row: usize,
}

impl ClipLocation {
    pub fn new(column: usize, row: usize) -> Self {
        Self { column, row }
    }
}
