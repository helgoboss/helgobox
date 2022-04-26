use crate::main::history::History;
use crate::main::row::Row;
use crate::main::{Clip, Column};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, AudioRecordingEquipment, ChainEquipment,
    ChainPreBufferCommandProcessor, MidiRecordingEquipment, QuantizationSettings, RecorderRequest,
    RecordingEquipment,
};
use crate::rt::{
    ClipChangedEvent, ClipPlayState, ColumnHandle, ColumnPlayClipArgs, ColumnPlayClipOptions,
    ColumnPlayRowArgs, ColumnStopArgs, ColumnStopClipArgs, OverridableMatrixSettings,
    QualifiedClipChangedEvent, RtMatrixCommandSender, WeakColumn,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use helgoboss_midi::Channel;
use playtime_api as api;
use playtime_api::{
    ChannelRange, ClipPlayStartTiming, ClipPlayStopTiming, ColumnPlayMode, Db,
    MatrixClipPlayAudioSettings, MatrixClipPlaySettings, MatrixClipRecordSettings, TempoRange,
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
    history: History,
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
            history: History::default(),
            _worker_pool: worker_pool,
        }
    }

    pub fn real_time_matrix(&self) -> rt::WeakMatrix {
        self.rt_matrix.downgrade()
    }

    pub fn load(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
        self.load_internal(api_matrix)?;
        self.history.clear();
        Ok(())
    }

    // TODO-medium We might be able to improve that to take API matrix by reference. This would
    //  slightly benefit undo/redo performance.
    fn load_internal(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
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
            column.sync_settings_to_rt(&self.settings);
            initialize_new_column(i, column, &self.rt_command_sender, &mut self.columns);
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

    fn clear_columns(&mut self) {
        // TODO-medium How about suspension?
        self.columns.clear();
        self.rt_command_sender.clear_columns();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> ClipEngineResult<()> {
        let api_matrix = self.history.undo()?.clone();
        self.load_internal(api_matrix)?;
        Ok(())
    }

    pub fn redo(&mut self) -> ClipEngineResult<()> {
        let api_matrix = self.history.redo()?.clone();
        self.load_internal(api_matrix)?;
        Ok(())
    }

    fn undoable<R>(&mut self, label: impl Into<String>, f: impl FnOnce(&mut Self) -> R) -> R {
        let owned_label = label.into();
        self.history
            .add(format!("Before {}", owned_label), self.save());
        let result = f(self);
        self.history.add(owned_label, self.save());
        result
    }

    /// Takes the current effective matrix dimensions into account, so even if a slot doesn't exist
    /// yet physically in the column, it returns `true` if it should exist.
    pub fn slot_exists(&self, coordinates: ClipSlotCoordinates) -> bool {
        coordinates.column < self.columns.len() && coordinates.row < self.row_count()
    }

    pub fn clip(&self, coordinates: ClipSlotCoordinates) -> Option<&Clip> {
        self.columns.get(coordinates.column)?.clip(coordinates.row)
    }

    pub fn all_clips_in_scene(
        &self,
        row_index: usize,
    ) -> impl Iterator<Item = ClipWithColumn> + '_ {
        let project = self.permanent_project();
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.follows_scene())
            .filter_map(move |(i, c)| {
                let clip = c.clip(row_index)?;
                let api_clip = clip.save(project).ok()?;
                Some(ClipWithColumn::new(i, api_clip))
            })
    }

    pub fn clear_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        if row_index >= self.row_count() {
            return Err("row doesn't exist");
        }
        self.history
            .add("Before clearing scene".to_owned(), self.save());
        // TODO-medium This is not optimal because it will create multiple undo points.
        for column in self.scene_columns() {
            column.clear_slot(row_index);
        }
        Ok(())
    }

    fn scene_columns(&self) -> impl Iterator<Item = &Column> {
        self.columns.iter().filter(|c| c.follows_scene())
    }

    pub fn clear_slot(&mut self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        // The undo point after clip removal is created later, in response to the upcoming event
        // that indicates that the slot has actually been cleared.
        self.history
            .add("Before clip removal".to_owned(), self.save());
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

    pub fn fill_row_with_clips(
        &mut self,
        row_index: usize,
        clips: Vec<ClipWithColumn>,
    ) -> ClipEngineResult<()> {
        self.undoable("Fill row with clips", |matrix| {
            for clip in clips {
                let column = match get_column_mut(&mut matrix.columns, clip.column_index).ok() {
                    None => break,
                    Some(c) => c,
                };
                column.fill_slot_with_clip(
                    row_index,
                    clip.clip,
                    &matrix.chain_equipment,
                    &matrix.recorder_request_sender,
                    &matrix.settings,
                )?;
            }
            matrix.handler.emit_event(ClipMatrixEvent::AllClipsChanged);
            Ok(())
        })
    }

    pub fn fill_slot_with_clip(
        &mut self,
        coordinates: ClipSlotCoordinates,
        api_clip: api::Clip,
    ) -> ClipEngineResult<()> {
        self.undoable("Fill slot with clip", |matrix| {
            let column = get_column_mut(&mut matrix.columns, coordinates.column)?;
            let event = column.fill_slot_with_clip(
                coordinates.row,
                api_clip,
                &matrix.chain_equipment,
                &matrix.recorder_request_sender,
                &matrix.settings,
            )?;
            matrix
                .handler
                .emit_event(ClipMatrixEvent::clip_changed(coordinates, event));
            Ok(())
        })
    }

    pub fn fill_slot_with_selected_item(
        &mut self,
        coordinates: ClipSlotCoordinates,
    ) -> ClipEngineResult<()> {
        self.undoable("Fill slot with selected item", |matrix| {
            let column = get_column_mut(&mut matrix.columns, coordinates.column)?;
            column.fill_slot_with_selected_item(
                coordinates.row,
                &matrix.chain_equipment,
                &matrix.recorder_request_sender,
                &matrix.settings,
            )
        })
    }

    pub fn play_clip(
        &self,
        coordinates: ClipSlotCoordinates,
        options: ColumnPlayClipOptions,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, coordinates.column)?;
        let args = ColumnPlayClipArgs {
            slot_index: coordinates.row,
            timeline,
            ref_pos: None,
            options,
        };
        column.play_clip(args);
        Ok(())
    }

    pub fn stop_clip(
        &self,
        coordinates: ClipSlotCoordinates,
        stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, coordinates.column)?;
        let args = ColumnStopClipArgs {
            slot_index: coordinates.row,
            timeline,
            ref_pos: None,
            stop_timing,
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

    pub fn play_row(&self, index: usize) {
        let timeline = self.timeline();
        let timeline_cursor_pos = timeline.cursor_pos();
        let args = ColumnPlayRowArgs {
            slot_index: index,
            timeline,
            ref_pos: timeline_cursor_pos,
        };
        for c in &self.columns {
            c.play_row(args.clone());
        }
    }

    pub fn build_scene_in_first_empty_row(&mut self) -> ClipEngineResult<()> {
        let empty_row_index = (0usize..)
            .find(|row_index| self.scene_is_empty(*row_index))
            .expect("there's always an empty row");
        self.build_scene_internal(empty_row_index)
    }

    pub fn build_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        if !self.scene_is_empty(row_index) {
            return Err("row is not empty");
        }
        self.build_scene_internal(row_index)
    }

    fn build_scene_internal(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Build scene", |matrix| {
            let playing_clips = matrix.capture_playing_clips()?;
            matrix.distribute_clips_to_scene(playing_clips, row_index)?;
            matrix.handler.emit_event(ClipMatrixEvent::AllClipsChanged);
            Ok(())
        })
    }

    fn distribute_clips_to_scene(
        &mut self,
        clips: Vec<ClipWithColumn>,
        row_index: usize,
    ) -> ClipEngineResult<()> {
        for clip in clips {
            // First try to put it within same column as clip itself
            let original_column = get_column_mut(&mut self.columns, clip.column_index)?;
            let dest_column =
                if original_column.follows_scene() && original_column.slot_is_empty(row_index) {
                    // We have space in that column, good.
                    original_column
                } else {
                    // We need to find another appropriate column.
                    let original_column_track = original_column.playback_track().ok().cloned();
                    let existing_column = self.columns.iter_mut().find(|c| {
                        c.follows_scene()
                            && c.slot_is_empty(row_index)
                            && c.playback_track().ok() == original_column_track.as_ref()
                    });
                    if let Some(c) = existing_column {
                        // Found.
                        c
                    } else {
                        // Not found. Create a new one.
                        let same_column = self.columns.get(clip.column_index).unwrap();
                        let mut duplicate = same_column.duplicate_without_contents();
                        duplicate.set_play_mode(ColumnPlayMode::ExclusiveFollowingScene);
                        duplicate.sync_settings_to_rt(&self.settings);
                        let duplicate_index = self.columns.len();
                        initialize_new_column(
                            duplicate_index,
                            duplicate,
                            &self.rt_command_sender,
                            &mut self.columns,
                        );
                        self.columns.last_mut().unwrap()
                    }
                };
            dest_column.fill_slot_with_clip(
                row_index,
                clip.clip,
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
            )?;
        }
        Ok(())
    }

    pub fn slot_is_empty(&self, coordinates: ClipSlotCoordinates) -> bool {
        let column = match self.columns.get(coordinates.column) {
            None => return false,
            Some(c) => c,
        };
        column.slot_is_empty(coordinates.row)
    }

    pub fn scene_is_empty(&self, row_index: usize) -> bool {
        self.scene_columns().all(|c| c.slot_is_empty(row_index))
    }

    fn capture_playing_clips(&self) -> ClipEngineResult<Vec<ClipWithColumn>> {
        let project = self.permanent_project();
        self.columns
            .iter()
            .enumerate()
            .flat_map(|(col_index, col)| {
                col.playing_clips().map(move |(_, clip)| {
                    let api_clip = clip.save(project)?;
                    Ok(ClipWithColumn::new(col_index, api_clip))
                })
            })
            .collect()
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
        let events: Vec<_> = self
            .columns
            .iter_mut()
            .enumerate()
            .flat_map(|(column_index, column)| {
                column
                    .poll(timeline_tempo)
                    .into_iter()
                    .map(move |(row_index, event)| {
                        ClipMatrixEvent::clip_changed(
                            ClipSlotCoordinates::new(column_index, row_index),
                            event,
                        )
                    })
            })
            .collect();
        let undo_point_label = if events.iter().any(|evt| evt.is_clip_removal()) {
            Some("Clip removed")
        } else if events.iter().any(|evt| evt.is_clip_recording_finished()) {
            Some("Clip recorded")
        } else {
            None
        };
        if let Some(l) = undo_point_label {
            self.history.add(l.into(), self.save());
        }
        events
    }

    pub fn toggle_looped(&mut self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        self.undoable("Toggle looped", |matrix| {
            let event = get_column_mut(&mut matrix.columns, coordinates.column())?
                .toggle_clip_looped(coordinates.row())?;
            matrix
                .handler
                .emit_event(ClipMatrixEvent::clip_changed(coordinates, event));
            Ok(())
        })
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

    pub fn column_is_armed_for_recording(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_armed_for_recording())
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
        if self.is_recording() {
            return Err("recording already");
        }
        self.history
            .add("Before clip recording".into(), self.save());
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

    pub fn is_recording(&self) -> bool {
        self.columns.iter().any(|c| c.is_recording())
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
        let event = get_column_mut(&mut self.columns, coordinates.column())?
            .set_clip_volume(coordinates.row(), volume)?;
        self.handler
            .emit_event(ClipMatrixEvent::clip_changed(coordinates, event));
        Ok(())
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

impl ClipMatrixEvent {
    pub fn clip_changed(slot_coordinates: ClipSlotCoordinates, event: ClipChangedEvent) -> Self {
        Self::ClipChanged(QualifiedClipChangedEvent {
            slot_coordinates,
            event,
        })
    }

    pub fn is_clip_removal(&self) -> bool {
        matches!(
            self,
            Self::ClipChanged(QualifiedClipChangedEvent {
                event: ClipChangedEvent::Removed,
                ..
            })
        )
    }

    pub fn is_clip_recording_finished(&self) -> bool {
        matches!(
            self,
            Self::ClipChanged(QualifiedClipChangedEvent {
                event: ClipChangedEvent::RecordingFinished,
                ..
            })
        )
    }
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
    pub stop_column_if_slot_empty: bool,
    pub play_start_timing: Option<ClipPlayStartTiming>,
    pub play_stop_timing: Option<ClipPlayStopTiming>,
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

#[derive(Clone, Debug)]
pub struct ClipWithColumn {
    column_index: usize,
    clip: api::Clip,
}

impl ClipWithColumn {
    fn new(column_index: usize, clip: api::Clip) -> Self {
        Self { column_index, clip }
    }
}

fn initialize_new_column(
    column_index: usize,
    column: Column,
    rt_command_sender: &Sender<rt::MatrixCommand>,
    columns: &mut Vec<Column>,
) {
    let handle = ColumnHandle {
        pointer: column.rt_column(),
        command_sender: column.rt_command_sender(),
    };
    rt_command_sender.insert_column(column_index, handle);
    columns.push(column);
}
