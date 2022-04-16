use crate::main::row::Row;
use crate::main::{Column, Slot};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, AudioRecordingEquipment, ChainEquipment,
    ChainPreBufferCommandProcessor, MidiRecordingEquipment, QuantizationSettings, RecorderRequest,
    RecordingEquipment,
};
use crate::rt::{
    ClipPlayState, ColumnHandle, ColumnPlayClipArgs, ColumnStopArgs, ColumnStopClipArgs,
    OverridableMatrixSettings, QualifiedClipChangedEvent, RtMatrixCommandSender, WeakColumn,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use helgoboss_midi::Channel;
use playtime_api as api;
use playtime_api::{
    ChannelRange, Db, MatrixClipPlayAudioSettings, MatrixClipPlaySettings,
    MatrixClipRecordSettings, TempoRange,
};
use reaper_high::{OrCurrentProject, Project, Reaper, Track};
use reaper_medium::{Bpm, MidiInputDeviceId, PositionInSeconds};
use std::thread::JoinHandle;
use std::{cmp, thread};

#[derive(Debug)]
pub struct Matrix<H> {
    /// Don't lock this from the main thread, only from real-time threads!
    rt_matrix: rt::SharedMatrix,
    settings: MatrixSettings,
    handler: H,
    chain_equipment: ChainEquipment,
    recorder_request_sender: Sender<RecorderRequest>,
    columns: Vec<Column>,
    rows: Vec<Row>,
    containing_track: Option<Track>,
    command_receiver: Receiver<MatrixCommand>,
    rt_command_sender: Sender<rt::MatrixCommand>,
    // We use this just for RAII (joining worker threads when dropped)
    _worker_pool: WorkerPool,
}

#[derive(Debug, Default)]
pub struct MatrixSettings {
    pub common_tempo_range: TempoRange,
    pub clip_record_settings: MatrixClipRecordSettings,
    pub overridable: OverridableMatrixSettings,
}

#[derive(Debug)]
pub enum MatrixCommand {
    ThrowAway(ColumnHandle),
}

pub trait MainMatrixCommandSender {
    fn throw_away(&self, handle: ColumnHandle);
    fn send_command(&self, command: MatrixCommand);
}

impl MainMatrixCommandSender for Sender<MatrixCommand> {
    fn throw_away(&self, handle: ColumnHandle) {
        self.send_command(MatrixCommand::ThrowAway(handle));
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
        let (recorder_request_sender, recorder_request_receiver) = crossbeam_channel::bounded(500);
        let (cache_request_sender, cache_request_receiver) = crossbeam_channel::bounded(500);
        let (pre_buffer_request_sender, pre_buffer_request_receiver) =
            crossbeam_channel::bounded(500);
        let (rt_command_sender, rt_command_receiver) = crossbeam_channel::bounded(500);
        let (main_command_sender, main_command_receiver) = crossbeam_channel::bounded(500);
        let mut worker_pool = WorkerPool::default();
        worker_pool.add_worker("Playtime recording worker", move || {
            keep_processing_recorder_requests(recorder_request_receiver);
        });
        worker_pool.add_worker("Playtime cache worker", move || {
            keep_processing_cache_requests(cache_request_receiver);
        });
        worker_pool.add_worker("Playtime pre-buffer worker", move || {
            keep_processing_pre_buffer_requests(
                pre_buffer_request_receiver,
                ChainPreBufferCommandProcessor,
            );
        });
        let project = containing_track.as_ref().map(|t| t.project());
        let rt_matrix = rt::Matrix::new(rt_command_receiver, main_command_sender, project);
        Self {
            rt_matrix: rt::SharedMatrix::new(rt_matrix),
            settings: Default::default(),
            handler,
            chain_equipment: ChainEquipment {
                cache_request_sender,
                pre_buffer_request_sender,
            },
            recorder_request_sender,
            columns: vec![],
            rows: vec![],
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
        // Main settings
        self.settings.common_tempo_range = api_matrix.common_tempo_range;
        self.settings.overridable.audio_resample_mode =
            api_matrix.clip_play_settings.audio_settings.resample_mode;
        self.settings.overridable.audio_time_stretch_mode = api_matrix
            .clip_play_settings
            .audio_settings
            .time_stretch_mode;
        self.settings.overridable.audio_cache_behavior =
            api_matrix.clip_play_settings.audio_settings.cache_behavior;
        self.settings.clip_record_settings = api_matrix.clip_record_settings;
        // Real-time settings
        self.settings.overridable.clip_play_start_timing =
            api_matrix.clip_play_settings.start_timing;
        self.settings.overridable.clip_play_stop_timing = api_matrix.clip_play_settings.stop_timing;
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
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
            )?;
            let handle = ColumnHandle {
                pointer: column.rt_column(),
                command_sender: column.rt_command_sender(),
            };
            self.rt_command_sender.insert_column(i, handle);
            self.columns.push(column);
        }
        // Rows
        self.rows = api_matrix
            .rows
            .unwrap_or_default()
            .into_iter()
            .map(|_| Row {})
            .collect();
        // Emit event
        self.handler.emit_event(ClipMatrixEvent::AllClipsChanged);
        Ok(())
    }

    pub fn save(&self) -> api::Matrix {
        api::Matrix {
            columns: Some(self.columns.iter().map(|column| column.save()).collect()),
            rows: Some(self.rows.iter().map(|row| row.save()).collect()),
            clip_play_settings: MatrixClipPlaySettings {
                start_timing: self.settings.overridable.clip_play_start_timing,
                stop_timing: self.settings.overridable.clip_play_stop_timing,
                audio_settings: MatrixClipPlayAudioSettings {
                    resample_mode: self.settings.overridable.audio_resample_mode,
                    time_stretch_mode: self.settings.overridable.audio_time_stretch_mode,
                    cache_behavior: self.settings.overridable.audio_cache_behavior,
                },
            },
            clip_record_settings: self.settings.clip_record_settings,
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

    /// Definitely returns a slot as long as it is within the current matrix bounds (even if empty).
    ///
    /// Returns `None` if out of current matrix bounds.
    pub fn slot(&mut self, coordinates: ClipSlotCoordinates) -> Option<&Slot> {
        let row_count = self.row_count();
        let column = get_column_mut(&mut self.columns, coordinates.column).ok()?;
        column.slot(coordinates.row(), row_count)
    }

    pub fn clear_slot(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, coordinates.column)?;
        column.clear_slot(coordinates.row);
        Ok(())
    }

    pub fn start_editing_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, coordinates.column)?;
        column.start_editing_clip(coordinates.row)
    }

    pub fn stop_editing_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, coordinates.column)?;
        column.stop_editing_clip(coordinates.row)
    }

    pub fn is_editing_clip(&self, coordinates: ClipSlotCoordinates) -> bool {
        if let Some(column) = self.columns.get(coordinates.column) {
            column.is_editing_clip(coordinates.row)
        } else {
            false
        }
    }

    pub fn fill_slot_with_selected_item(
        &mut self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<()> {
        let column = get_column_mut(&mut self.columns, coordinates.column)?;
        column.fill_slot_with_selected_item(
            coordinates.row,
            &self.chain_equipment,
            &self.recorder_request_sender,
            &self.settings,
        )
    }

    pub fn play_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, coordinates.column)?;
        let args = ColumnPlayClipArgs {
            slot_index: coordinates.row,
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
            timeline,
            ref_pos: None,
        };
        column.stop_clip(args);
        Ok(())
    }

    pub fn stop(&self) {
        let timeline = self.timeline();
        let args = ColumnStopArgs {
            ref_pos: Some(timeline.cursor_pos()),
            timeline,
        };
        for c in &self.columns {
            c.stop(args.clone());
        }
    }

    pub fn stop_column(&self, index: usize) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, index)?;
        let args = ColumnStopArgs {
            timeline,
            ref_pos: None,
        };
        column.stop(args);
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
        get_column(&self.columns, coordinates.column())?.slot_position_in_seconds(coordinates.row())
    }

    pub fn is_stoppable(&self) -> bool {
        self.columns.iter().any(|c| c.is_stoppable())
    }

    pub fn column_is_stoppable(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_stoppable())
            .unwrap_or(false)
    }

    pub fn set_column_armed_for_recording(
        &self,
        index: usize,
        armed: bool,
    ) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, index)?;
        column.set_armed_for_recording(armed)?;
        Ok(())
    }

    pub fn column_is_armed_for_recording(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_armed_for_recording())
            .unwrap_or(false)
    }

    pub fn set_column_solo(&self, index: usize, solo: bool) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, index)?;
        column.set_solo(solo)?;
        Ok(())
    }

    pub fn column_is_solo(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_solo())
            .unwrap_or(false)
    }

    pub fn set_column_mute(&self, index: usize, mute: bool) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, index)?;
        column.set_mute(mute)?;
        Ok(())
    }

    pub fn column_is_mute(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_mute())
            .unwrap_or(false)
    }

    pub fn set_column_selected(&self, index: usize, selected: bool) -> ClipEngineResult<()> {
        let column = get_column(&self.columns, index)?;
        column.set_selected(selected)?;
        Ok(())
    }

    pub fn column_is_selected(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_selected())
            .unwrap_or(false)
    }

    pub fn clip_play_state(
        &self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<ClipPlayState> {
        get_column(&self.columns, coordinates.column())?.clip_play_state(coordinates.row())
    }

    pub fn clip_looped(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<bool> {
        get_column(&self.columns, coordinates.column())?.clip_looped(coordinates.row())
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    pub fn column(&self, index: usize) -> ClipEngineResult<&Column> {
        get_column(&self.columns, index)
    }

    pub fn row_count(&self) -> usize {
        let max_slot_count_per_col = self
            .columns
            .iter()
            .map(|c| c.slot_count())
            .max()
            .unwrap_or(0);
        cmp::max(self.rows.len(), max_slot_count_per_col)
    }

    pub fn clip_volume(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<Db> {
        get_column(&self.columns, coordinates.column())?.clip_volume(coordinates.row())
    }

    pub fn record_clip(&mut self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        get_column_mut(&mut self.columns, coordinates.column())?.record_clip(
            coordinates.row(),
            &self.settings.clip_record_settings,
            &self.chain_equipment,
            &self.recorder_request_sender,
            &self.handler,
            self.containing_track.as_ref(),
            &self.settings.overridable,
        )
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
            .proportional_slot_position(coordinates.row())
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
    pub input: ClipRecordInput,
    pub destination: ClipRecordDestination,
}

#[derive(Debug)]
pub struct ClipRecordDestination {
    pub column_source: WeakColumn,
    pub slot_index: usize,
    /// If this is set, it's important to write the MIDI events during the *post* phase of the audio
    /// callback, otherwise the written MIDI events would be played back a moment later, which
    /// would result in duplicated note playback during recording.
    ///
    /// If this is not set, it's important to write it in the *pre* phase because we don't want
    /// to miss playing back any material when we change back from recording to ready.
    pub is_midi_overdub: bool,
}

#[derive(Debug)]
pub enum ClipRecordInput {
    HardwareInput(ClipRecordHardwareInput),
    FxInput(VirtualClipRecordAudioInput),
}

impl ClipRecordInput {
    /// Project is necessary to create an audio sink.
    pub fn create_recording_equipment(
        &self,
        project: Option<Project>,
        auto_quantize_midi: bool,
    ) -> ClipEngineResult<RecordingEquipment> {
        use ClipRecordInput::*;
        match &self {
            HardwareInput(ClipRecordHardwareInput::Midi(_)) => {
                let quantization_settings = if auto_quantize_midi {
                    // TODO-high Use project quantization settings
                    Some(QuantizationSettings {})
                } else {
                    None
                };
                let equipment = MidiRecordingEquipment::new(quantization_settings);
                Ok(RecordingEquipment::Midi(equipment))
            }
            HardwareInput(ClipRecordHardwareInput::Audio(virtual_input))
            | FxInput(virtual_input) => {
                let channel_count = match virtual_input {
                    VirtualClipRecordAudioInput::Specific(range) => range.channel_count,
                    VirtualClipRecordAudioInput::Detect { channel_count } => *channel_count,
                };
                let sample_rate = Reaper::get().audio_device_sample_rate()?;
                let equipment =
                    AudioRecordingEquipment::new(project, channel_count as _, sample_rate);
                Ok(RecordingEquipment::Audio(equipment))
            }
        }
    }
}

#[derive(Debug)]
pub enum ClipRecordHardwareInput {
    Midi(VirtualClipRecordHardwareMidiInput),
    Audio(VirtualClipRecordAudioInput),
}

#[derive(Debug)]
pub enum VirtualClipRecordHardwareMidiInput {
    Specific(ClipRecordHardwareMidiInput),
    Detect,
}

#[derive(Copy, Clone, Debug)]
pub struct ClipRecordHardwareMidiInput {
    pub device_id: Option<MidiInputDeviceId>,
    pub channel: Option<Channel>,
}

#[derive(Debug)]
pub enum VirtualClipRecordAudioInput {
    Specific(ChannelRange),
    Detect { channel_count: u32 },
}

impl VirtualClipRecordAudioInput {
    pub fn channel_offset(&self) -> ClipEngineResult<u32> {
        use VirtualClipRecordAudioInput::*;
        match self {
            Specific(channel_range) => Ok(channel_range.first_channel_index),
            Detect { .. } => Err("audio input detection not yet implemented"),
        }
    }
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

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct ClipTransportOptions {
    /// If this is on and one of the record actions is triggered, it will only have an effect if
    /// the record track of the clip column is armed.
    pub record_only_if_track_armed: bool,
    // pub use_empty_slots_for_column_stop: bool,
}

#[derive(Copy, Clone)]
pub struct RecordArgs {
    pub kind: RecordKind,
}

#[derive(Copy, Clone)]
pub enum RecordKind {
    Normal {
        looped: bool,
        timing: ClipRecordTiming,
        detect_downbeat: bool,
    },
    MidiOverdub,
}
