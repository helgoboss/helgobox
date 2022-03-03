use crate::main::{Column, Slot};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, keep_stretching, RecorderEquipment, StretchWorkerRequest,
};
use crate::rt::{
    ClipPlayState, ColumnPlayClipArgs, ColumnStopClipArgs, QualifiedClipChangedEvent,
    RecordBehavior, RecordTiming, RtMatrixCommandSender, SharedColumn, WeakColumn,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, ClipRecordStartTiming, ClipRecordStopTiming,
    ClipRecordTimeBase, ClipSettingOverrideAfterRecording, Db, MatrixClipPlayAudioSettings,
    MatrixClipPlaySettings, MatrixClipRecordAudioSettings, MatrixClipRecordMidiSettings,
    MatrixClipRecordSettings, MidiClipRecordMode, RecordLength, TempoRange, VirtualResampleMode,
};
use reaper_high::{OrCurrentProject, Project, Track};
use reaper_medium::{Bpm, PositionInSeconds};
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug)]
pub struct Matrix<H> {
    /// Don't lock this from the main thread, only from real-time threads!
    rt_matrix: rt::SharedMatrix,
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
    ThrowAway(WeakColumn),
}

pub trait MainMatrixCommandSender {
    fn throw_away(&self, source: WeakColumn);
    fn send_command(&self, command: MatrixCommand);
}

impl MainMatrixCommandSender for Sender<MatrixCommand> {
    fn throw_away(&self, source: WeakColumn) {
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
    pub fn new(handler: H, containing_track: Option<Track>) -> Self {
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
        let rt_matrix = rt::Matrix::new(rt_command_receiver, main_command_sender, project);
        Self {
            rt_matrix: rt::SharedMatrix::new(rt_matrix),
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
        }
    }

    pub fn real_time_matrix(&self) -> rt::WeakMatrix {
        self.rt_matrix.downgrade()
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
        self.handler.emit_event(ClipMatrixEvent::AllClipsChanged);
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

    pub fn slot(&self, coordinates: ClipSlotCoordinates) -> Option<&Slot> {
        let column = get_column(&self.columns, coordinates.column).ok()?;
        column.slot(coordinates.row())
    }

    pub fn play_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, coordinates.column)?;
        let args = ColumnPlayClipArgs {
            slot_index: coordinates.row,
            parent_start_timing: self.rt_settings.clip_play_start_timing,
            parent_stop_timing: self.rt_settings.clip_play_stop_timing,
            timeline,
            ref_pos: None,
        };
        column.play_clip(args);
        Ok(())
    }

    pub fn stop_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, coordinates.column)?;
        let args = ColumnStopClipArgs {
            slot_index: coordinates.row,
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

    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<ClipMatrixEvent> {
        self.process_commands();
        self.columns
            .iter_mut()
            .enumerate()
            .flat_map(|(column_index, column)| {
                column
                    .poll(timeline_tempo)
                    .into_iter()
                    .map(move |(row_index, event)| {
                        ClipMatrixEvent::ClipChanged(QualifiedClipChangedEvent {
                            slot_coordinates: ClipSlotCoordinates::new(column_index, row_index),
                            event,
                        })
                    })
            })
            .collect()
    }

    pub fn toggle_looped(&mut self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let event = get_column_mut(&mut self.columns, coordinates.column())?
            .toggle_clip_looped(coordinates.row())?;
        let event = ClipMatrixEvent::ClipChanged(QualifiedClipChangedEvent {
            slot_coordinates: coordinates,
            event,
        });
        self.handler.emit_event(event);
        Ok(())
    }

    pub fn clip_position_in_seconds(
        &self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<PositionInSeconds> {
        get_column(&self.columns, coordinates.column())?.clip_position_in_seconds(coordinates.row())
    }

    pub fn clip_play_state(
        &self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<ClipPlayState> {
        get_column(&self.columns, coordinates.column())?.clip_play_state(coordinates.row())
    }

    pub fn clip_repeated(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<bool> {
        get_column(&self.columns, coordinates.column())?.clip_repeated(coordinates.row())
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn clip_volume(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<Db> {
        get_column(&self.columns, coordinates.column())?.clip_volume(coordinates.row())
    }

    pub fn record_clip_legacy(
        &mut self,
        coordinates: ClipSlotCoordinates,
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
        let task = get_column_mut(&mut self.columns, coordinates.column())?.record_clip(
            coordinates.row(),
            behavior,
            self.recorder_equipment.clone(),
        )?;
        self.handler.request_recording_input(task);
        Ok(())
    }

    pub fn pause_clip_legacy(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        get_column(&self.columns, coordinates.column())?.pause_clip(coordinates.row());
        Ok(())
    }

    pub fn seek_clip_legacy(
        &self,
        coordinates: ClipSlotCoordinates,
        position: UnitValue,
    ) -> ClipEngineResult<()> {
        get_column(&self.columns, coordinates.column())?.seek_clip(coordinates.row(), position);
        Ok(())
    }

    pub fn set_clip_volume(
        &mut self,
        coordinates: ClipSlotCoordinates,
        volume: Db,
    ) -> ClipEngineResult<()> {
        get_column_mut(&mut self.columns, coordinates.column())?
            .set_clip_volume(coordinates.row(), volume)
    }

    pub fn proportional_clip_position(
        &self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<UnitValue> {
        get_column(&self.columns, coordinates.column())?
            .proportional_clip_position(coordinates.row())
    }
}

fn get_column(columns: &[Column], index: usize) -> ClipEngineResult<&Column> {
    columns.get(index).ok_or(NO_SUCH_COLUMN)
}

fn get_column_mut(columns: &mut [Column], index: usize) -> ClipEngineResult<&mut Column> {
    columns.get_mut(index).ok_or(NO_SUCH_COLUMN)
}

const NO_SUCH_COLUMN: &str = "no such column";

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ClipSlotCoordinates {
    column: usize,
    row: usize,
}

impl ClipSlotCoordinates {
    pub fn new(column: usize, row: usize) -> Self {
        Self { column, row }
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn row(&self) -> usize {
        self.row
    }
}

#[derive(Debug)]
pub struct ClipRecordTask {
    pub column_source: SharedColumn,
    pub slot_index: usize,
}

pub trait ClipMatrixHandler {
    fn request_recording_input(&self, task: ClipRecordTask);
    fn emit_event(&self, event: ClipMatrixEvent);
}

#[derive(Debug)]
pub enum ClipMatrixEvent {
    AllClipsChanged,
    ClipChanged(QualifiedClipChangedEvent),
}

#[derive(Copy, Clone, Debug)]
pub enum ClipRecordTiming {
    StartImmediatelyStopOnDemand,
    StartOnBarStopOnDemand { start_bar: i32 },
    StartOnBarStopOnBar { start_bar: i32, bar_count: u32 },
}

/// Contains instructions how to play a clip.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {}

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
