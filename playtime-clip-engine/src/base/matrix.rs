use crate::base::history::History;
use crate::base::row::Row;
use crate::base::{
    reorder_tracks, BoxedReaperChange, Clip, Column, ColumnRtEquipment,
    EssentialColumnRecordClipArgs, IdMode, ReaperChange, ReaperChangeContext,
    RestorationInstruction, Slot, SlotKit, TrackAdditionReaperChange, TrackReorderingReaperChange,
};
use crate::rt::audio_hook::{FxInputClipRecordTask, HardwareInputClipRecordTask};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, AudioRecordingEquipment, ChainEquipment,
    ChainPreBufferCommandProcessor, MidiRecordingEquipment, QuantizationSettings, RecorderRequest,
    RecordingEquipment,
};
use crate::rt::{
    ClipChangeEvent, ColumnHandle, ColumnPlayRowArgs, ColumnPlaySlotArgs, ColumnPlaySlotOptions,
    ColumnStopArgs, ColumnStopSlotArgs, FillSlotMode, OverridableMatrixSettings,
    QualifiedClipChangeEvent, QualifiedSlotChangeEvent, RtMatrixCommandSender, SlotChangeEvent,
    WeakRtColumn,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use base::validation_util::{ensure_no_duplicate, ValidationError};
use crossbeam_channel::{Receiver, Sender};
use derivative::Derivative;
use helgoboss_learn::UnitValue;
use helgoboss_midi::Channel;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    ChannelRange, ClipPlayStopTiming, ColumnId, Db, MatrixClipPlayAudioSettings,
    MatrixClipPlaySettings, MatrixClipRecordSettings, RecordLength, RowId, TempoRange, TrackId,
};
use reaper_high::{ChangeEvent, OrCurrentProject, Project, Reaper, Track};
use reaper_medium::{Bpm, MidiInputDeviceId};
use std::collections::HashMap;
use std::error::Error;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{mem, thread};

#[derive(Debug)]
pub struct Matrix {
    history: History,
    content: MatrixContent,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub(crate) struct MatrixContent {
    /// Don't lock this from the main thread, only from real-time threads!
    rt_matrix: rt::SharedRtMatrix,
    settings: MatrixSettings,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn ClipMatrixHandler>,
    chain_equipment: ChainEquipment,
    recorder_request_sender: Sender<RecorderRequest>,
    columns: Vec<Column>,
    /// Contains columns that are not in use anymore since the last column removal operation.
    ///
    /// They are still kept around in order to allow for a smooth fadeout of the clips on the
    /// removed column (instead of abruptly interrupt playing and therefore ending up with an
    /// annoying click).
    retired_columns: Vec<RetiredColumn>,
    rows: Vec<Row>,
    containing_track: Option<Track>,
    command_receiver: Receiver<MatrixCommand>,
    rt_command_sender: Sender<rt::RtMatrixCommand>,
    clipboard: MatrixClipboard,
    poll_count: u64,
    // We use this just for RAII (joining worker threads when dropped)
    _worker_pool: WorkerPool,
}

impl MatrixContent {
    pub fn sync_column_handles_to_rt_matrix(&mut self) {
        let column_handles = self.columns.iter().map(|c| c.create_handle()).collect();
        self.rt_command_sender.set_column_handles(column_handles);
    }

    pub fn permanent_project(&self) -> Option<Project> {
        self.containing_track.as_ref().map(|t| t.project())
    }

    /// Used by preset loading and by undo/redo.
    pub fn load_internal(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
        let permanent_project = self.permanent_project();
        let necessary_row_count = api_matrix.necessary_row_count();
        // Settings
        self.settings = MatrixSettings::from_api(&api_matrix);
        // Columns
        let mut old_columns: HashMap<_, _> = mem::take(&mut self.columns)
            .into_iter()
            .map(|c| (c.id().clone(), c))
            .collect();
        for api_column in api_matrix.columns.unwrap_or_default().into_iter() {
            let mut column = old_columns.remove(&api_column.id).unwrap_or_else(|| {
                Column::new(
                    api_column.id.clone(),
                    permanent_project,
                    necessary_row_count,
                )
            });
            column.load(
                api_column,
                necessary_row_count,
                ColumnRtEquipment {
                    chain_equipment: &self.chain_equipment,
                    recorder_request_sender: &self.recorder_request_sender,
                    matrix_settings: &self.settings,
                },
            )?;
            self.columns.push(column);
        }
        // Rows
        self.rows = api_matrix
            .rows
            .unwrap_or_default()
            .into_iter()
            .map(Row::from_api_row)
            .collect();
        self.rows
            .resize_with(necessary_row_count, || Row::new(RowId::random()));
        // Sync to real-time matrix
        self.sync_column_handles_to_rt_matrix();
        // Retire old and now unused columns
        self.retired_columns
            .extend(old_columns.into_values().map(RetiredColumn::new));
        // Notify listeners
        self.notify_everything_changed();
        Ok(())
    }

    pub fn set_column_playback_track(
        &mut self,
        column_index: usize,
        track_id: Option<&TrackId>,
    ) -> ClipEngineResult<()> {
        let column = self.columns.get_mut(column_index).ok_or(NO_SUCH_COLUMN)?;
        column.set_playback_track_from_id(
            track_id,
            ColumnRtEquipment {
                chain_equipment: &self.chain_equipment,
                recorder_request_sender: &self.recorder_request_sender,
                matrix_settings: &self.settings,
            },
        )?;
        self.notify_everything_changed();
        Ok(())
    }

    pub fn notify_everything_changed(&self) {
        self.emit(ClipMatrixEvent::EverythingChanged);
    }

    pub fn emit(&self, event: ClipMatrixEvent) {
        self.handler.emit_event(event);
    }
}

#[derive(Debug)]
struct RetiredColumn {
    time_of_retirement: Instant,
    _column: Column,
}

impl RetiredColumn {
    pub fn new(column: Column) -> Self {
        Self {
            time_of_retirement: Instant::now(),
            _column: column,
        }
    }

    pub fn is_still_alive(&self) -> bool {
        const MAX_RETIRED_DURATION: Duration = Duration::from_millis(1000);
        self.time_of_retirement.elapsed() < MAX_RETIRED_DURATION
    }
}

#[derive(Debug, Default)]
struct MatrixClipboard {
    content: Option<MatrixClipboardContent>,
}

#[derive(Debug)]
enum MatrixClipboardContent {
    Slot(Vec<api::Clip>),
    Scene(Vec<SlotContentsWithColumn>),
}

#[derive(Debug, Default)]
pub struct MatrixSettings {
    pub common_tempo_range: TempoRange,
    pub clip_record_settings: MatrixClipRecordSettings,
    pub overridable: OverridableMatrixSettings,
}

impl MatrixSettings {
    pub fn from_api(matrix: &api::Matrix) -> Self {
        Self {
            common_tempo_range: matrix.common_tempo_range,
            clip_record_settings: matrix.clip_record_settings,
            overridable: OverridableMatrixSettings::from_api(&matrix.clip_play_settings),
        }
    }
}

#[derive(Debug)]
pub enum MatrixCommand {
    ThrowAway(MatrixGarbage),
}

#[derive(Debug)]
pub enum MatrixGarbage {
    ColumnHandles(Vec<ColumnHandle>),
}

pub trait MainMatrixCommandSender {
    fn throw_away(&self, garbage: MatrixGarbage);
    fn send_command(&self, command: MatrixCommand);
}

impl MainMatrixCommandSender for Sender<MatrixCommand> {
    fn throw_away(&self, garbage: MatrixGarbage) {
        self.send_command(MatrixCommand::ThrowAway(garbage));
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

impl Matrix {
    pub fn save(&self) -> api::Matrix {
        api::Matrix {
            columns: Some(
                self.content
                    .columns
                    .iter()
                    .map(|column| column.save())
                    .collect(),
            ),
            rows: Some(self.content.rows.iter().map(|row| row.save()).collect()),
            clip_play_settings: self.save_play_settings(),
            clip_record_settings: self.content.settings.clip_record_settings,
            common_tempo_range: self.content.settings.common_tempo_range,
        }
    }

    fn save_play_settings(&self) -> api::MatrixClipPlaySettings {
        MatrixClipPlaySettings {
            start_timing: self.content.settings.overridable.clip_play_start_timing,
            stop_timing: self.content.settings.overridable.clip_play_stop_timing,
            audio_settings: MatrixClipPlayAudioSettings {
                resample_mode: self.content.settings.overridable.audio_resample_mode,
                time_stretch_mode: self.content.settings.overridable.audio_time_stretch_mode,
                cache_behavior: self.content.settings.overridable.audio_cache_behavior,
            },
        }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Returns the project which contains this matrix, unless the matrix is project-less
    /// (monitoring FX chain).
    pub fn permanent_project(&self) -> Option<Project> {
        self.content.permanent_project()
    }

    /// Returns the permanent project. If the matrix is project-less, the current project.
    pub fn temporary_project(&self) -> Project {
        self.permanent_project().or_current_project()
    }

    /// Creates an empty matrix with no columns and no rows.
    pub fn new(handler: Box<dyn ClipMatrixHandler>, containing_track: Option<Track>) -> Self {
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
        let rt_matrix = rt::RtMatrix::new(rt_command_receiver, main_command_sender, project);
        Self {
            history: History::new(Default::default()),
            content: MatrixContent {
                rt_matrix: {
                    let m = rt::SharedRtMatrix::new(rt_matrix);
                    // This is necessary since Rust 1.62.0 (or 1.63.0, not sure). Since those versions,
                    // locking a mutex the first time apparently allocates. If we don't lock the
                    // mutex now for the first time but do it in the real-time thread, assert_no_alloc will
                    // complain in debug builds.
                    drop(m.lock());
                    m
                },
                settings: Default::default(),
                handler,
                chain_equipment: ChainEquipment {
                    cache_request_sender,
                    pre_buffer_request_sender,
                },
                recorder_request_sender,
                columns: vec![],
                retired_columns: vec![],
                rows: vec![],
                containing_track,
                command_receiver: main_command_receiver,
                rt_command_sender,
                clipboard: Default::default(),
                poll_count: 0,
                _worker_pool: worker_pool,
            },
        }
    }

    pub fn real_time_matrix(&self) -> rt::WeakRtMatrix {
        self.content.rt_matrix.downgrade()
    }

    pub fn load(&mut self, api_matrix: api::Matrix) -> Result<(), Box<dyn Error>> {
        // We make a fresh start by throwing away all existing columns (with preview registers).
        // In theory, we don't need to do this because our core loading logic (the same that we also
        // use for undo/redo) should ideally be solid enough to react correctly when loading
        // something completely different. But who knows, maybe we have bugs in there and with the
        // following simple line we can effectively prevent those from having an effect here.
        self.content
            .retired_columns
            .extend(self.content.columns.drain(..).map(RetiredColumn::new));
        // This is a public method so we need to expect the worst, also invalid data! Do checks.
        validate_api_matrix(&api_matrix)?;
        // Core loading logic
        self.content.load_internal(api_matrix)?;
        // We want to reset undo/redo history
        self.history = History::new(self.save());
        Ok(())
    }

    pub fn next_undo_label(&self) -> Option<&str> {
        self.history.next_undo_label()
    }

    pub fn next_redo_label(&self) -> Option<&str> {
        self.history.next_redo_label()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> ClipEngineResult<()> {
        self.restore_history_state(
            |history| history.undo(),
            |ch, context| ch.pre_undo(context),
            |ch, context| ch.post_undo(context),
        )
    }

    pub fn redo(&mut self) -> ClipEngineResult<()> {
        self.restore_history_state(
            |history| history.redo(),
            |ch, context| ch.pre_redo(context),
            |ch, context| ch.post_redo(context),
        )
    }

    fn restore_history_state(
        &mut self,
        pop_state: impl FnOnce(&mut History) -> ClipEngineResult<RestorationInstruction>,
        pre_load: impl Fn(&mut dyn ReaperChange, ReaperChangeContext) -> Result<(), Box<dyn Error>>,
        post_load: impl Fn(&mut dyn ReaperChange, ReaperChangeContext) -> Result<(), Box<dyn Error>>,
    ) -> ClipEngineResult<()> {
        let instruction = pop_state(&mut self.history)?;
        // Apply pre-load REAPER changes
        //
        // # Example 1: Insert column
        //
        // Undo => -
        // Redo => Insert REAPER track
        //
        // # Example 2: Remove column
        //
        // Undo => Insert REAPER track
        // Redo => -
        for change in instruction.reaper_changes.iter_mut() {
            let context = ReaperChangeContext {
                matrix: &mut self.content,
            };
            let _ = pre_load(&mut **change, context);
        }
        // Restore actual matrix state
        self.content.load_internal(instruction.matrix.clone())?;
        // Apply post-load REAPER changes
        //
        // # Example 1: Insert column
        //
        // Undo => Remove REAPER track
        // Redo => -
        //
        // # Example 2: Remove column
        //
        // Undo => -
        // Redo => Remove REAPER track
        for change in instruction.reaper_changes.iter_mut() {
            let context = ReaperChangeContext {
                matrix: &mut self.content,
            };
            let _ = post_load(&mut **change, context);
        }
        // Emit change notification
        self.emit(ClipMatrixEvent::HistoryChanged);
        Ok(())
    }

    fn undoable(
        &mut self,
        label: impl Into<String>,
        f: impl FnOnce(&mut Self) -> ClipEngineResult<Vec<BoxedReaperChange>>,
    ) -> ClipEngineResult<()> {
        let reaper_changes = f(self)?;
        self.history.add_buffered(label.into(), reaper_changes);
        Ok(())
    }

    /// Freezes the complete matrix.
    pub async fn freeze(&mut self) {
        for (i, column) in self.content.columns.iter_mut().enumerate() {
            let _ = column.freeze(i).await;
        }
    }

    /// Takes the current effective matrix dimensions into account, so even if a slot doesn't exist
    /// yet physically in the column, it returns `true` if it *should* exist.
    pub fn slot_exists(&self, coordinates: ClipSlotAddress) -> bool {
        coordinates.column < self.content.columns.len() && coordinates.row < self.row_count()
    }

    /// Finds the column at the given index.
    pub fn find_column(&self, index: usize) -> Option<&Column> {
        self.get_column(index).ok()
    }

    /// Finds the slot at the given address.
    pub fn find_slot(&self, address: ClipSlotAddress) -> Option<&Slot> {
        self.get_slot(address).ok()
    }

    /// Finds the clip at the given address.
    pub fn find_clip(&self, address: ClipAddress) -> Option<&Clip> {
        self.get_clip(address).ok()
    }

    /// Returns an iterator over all slots in each column, including the column indexes.
    pub fn all_slots(&self) -> impl Iterator<Item = SlotWithColumn> + '_ {
        self.content
            .columns
            .iter()
            .enumerate()
            .flat_map(|(column_index, column)| {
                column
                    .slots()
                    .map(move |slot| SlotWithColumn::new(column_index, slot))
            })
    }

    /// Returns an iterator over all columns.
    pub fn all_columns(&self) -> impl Iterator<Item = &Column> + '_ {
        self.content.columns.iter()
    }

    /// Returns an iterator over all clips in a row whose column is a scene follower.
    fn all_clips_in_scene(
        &self,
        row_index: usize,
    ) -> impl Iterator<Item = SlotContentsWithColumn> + '_ {
        let _project = self.permanent_project();
        self.content
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.follows_scene())
            .filter_map(move |(i, c)| Some((i, c.get_slot(row_index).ok()?)))
            .map(move |(i, s)| {
                let api_clips = s.clips().filter_map(move |clip| clip.save().ok());
                SlotContentsWithColumn::new(i, api_clips.collect())
            })
    }

    /// Cuts the given scene's clips to the matrix clipboard.
    pub fn cut_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.copy_scene(row_index)?;
        self.clear_scene(row_index)?;
        Ok(())
    }

    /// Copies the given scene's clips to the matrix clipboard.
    pub fn copy_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        let clips = self.all_clips_in_scene(row_index).collect();
        self.content.clipboard.content = Some(MatrixClipboardContent::Scene(clips));
        Ok(())
    }

    /// Pastes the clips stored in the matrix clipboard into the given scene.
    pub fn paste_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        let content = self
            .content
            .clipboard
            .content
            .as_ref()
            .ok_or("clipboard empty")?;
        let MatrixClipboardContent::Scene(clips) = content else {
            return Err("clipboard doesn't contain scene contents");
        };
        let cloned_clips = clips.clone();
        self.undoable("Fill row with clips", |matrix| {
            matrix.replace_row_with_clips(row_index, cloned_clips, IdMode::AssignNewIds)?;
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn remove_column(&mut self, column_index: usize) -> ClipEngineResult<()> {
        if column_index >= self.content.columns.len() {
            return Err("column doesn't exist");
        }
        self.undoable("Remove column", |matrix| {
            let column = matrix.content.columns.remove(column_index);
            column.panic();
            matrix
                .content
                .retired_columns
                .push(RetiredColumn::new(column));
            matrix.content.sync_column_handles_to_rt_matrix();
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn duplicate_column(&mut self, column_index: usize) -> ClipEngineResult<()> {
        self.undoable("Duplicate column", |matrix| {
            let column = matrix
                .content
                .columns
                .get(column_index)
                .ok_or("column doesn't exist")?;
            let duplicate_column = column.duplicate(ColumnRtEquipment {
                chain_equipment: &matrix.content.chain_equipment,
                recorder_request_sender: &matrix.content.recorder_request_sender,
                matrix_settings: &matrix.content.settings,
            });
            matrix
                .content
                .columns
                .insert(column_index + 1, duplicate_column);
            matrix.content.sync_column_handles_to_rt_matrix();
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    /// Determines the index of a new REAPER track to be added togeter with the given new column.
    ///
    /// Only works if the existing column next to it has a playback track assigned.
    fn determine_index_of_new_track(&self, new_column_index: usize) -> Option<u32> {
        if self.content.columns.is_empty() {
            // We add the first column. Add track at end.
            return Some(self.temporary_project().track_count());
        }
        let reference_column_index = if new_column_index < self.content.columns.len() {
            // Add column in the middle. Add track above playback track of column which was
            // previously at that position.
            new_column_index
        } else {
            // Add column at end. Add track below playback track of last column.
            new_column_index - 1
        };
        let reference_column = self.content.columns.get(reference_column_index)?;
        reference_column.playback_track().ok()?.index()
    }

    pub fn insert_column(&mut self, column_index: usize) -> ClipEngineResult<()> {
        // Try to add new column track in REAPER
        let new_track_index = self.determine_index_of_new_track(column_index);
        let new_track =
            new_track_index.and_then(|i| self.temporary_project().insert_track_at(i).ok());
        let reaper_changes: Vec<BoxedReaperChange> = if let Some(t) = &new_track {
            let reaper_change = TrackAdditionReaperChange::new(t, column_index);
            vec![Box::new(reaper_change)]
        } else {
            vec![]
        };
        // Add actual column to matrix, with or without new track
        self.undoable("Insert column", |matrix| {
            let mut new_column = Column::new(
                ColumnId::random(),
                matrix.permanent_project(),
                matrix.content.rows.len(),
            );
            new_column.set_playback_track(new_track, matrix.column_rt_equipment());
            matrix.content.columns.insert(column_index, new_column);
            matrix.content.sync_column_handles_to_rt_matrix();
            matrix.notify_everything_changed();
            Ok(reaper_changes)
        })
    }

    pub fn duplicate_row(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Duplicate row", |matrix| {
            let row = matrix
                .content
                .rows
                .get(row_index)
                .ok_or("row doesn't exist")?;
            matrix.content.rows.insert(row_index + 1, row.duplicate());
            for column in &mut matrix.content.columns {
                column.duplicate_slot(
                    row_index,
                    ColumnRtEquipment {
                        chain_equipment: &matrix.content.chain_equipment,
                        recorder_request_sender: &matrix.content.recorder_request_sender,
                        matrix_settings: &matrix.content.settings,
                    },
                )?;
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn insert_row(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Insert row", |matrix| {
            if row_index > matrix.content.rows.len() {
                return Err("row index too large");
            }
            matrix
                .content
                .rows
                .insert(row_index, Row::new(RowId::random()));
            for column in &mut matrix.content.columns {
                column.insert_slot(
                    row_index,
                    ColumnRtEquipment {
                        chain_equipment: &matrix.content.chain_equipment,
                        recorder_request_sender: &matrix.content.recorder_request_sender,
                        matrix_settings: &matrix.content.settings,
                    },
                )?;
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn remove_row(&mut self, row_index: usize) -> ClipEngineResult<()> {
        if row_index >= self.row_count() {
            return Err("row doesn't exist");
        }
        self.undoable("Remove row", |matrix| {
            matrix.content.rows.remove(row_index);
            for column in &mut matrix.content.columns {
                // It's possible that the slot index doesn't exist in that column because slots
                // are added lazily.
                column.remove_slot(row_index)?;
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    /// Clears the slots of all scene-following columns.
    pub fn clear_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Clear scene", |matrix| {
            matrix.clear_scene_internal(row_index)?;
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    fn clear_scene_internal(&mut self, row_index: usize) -> ClipEngineResult<()> {
        if row_index >= self.row_count() {
            return Err("row doesn't exist");
        }
        for column in self.scene_columns_mut() {
            // If the slot doesn't exist in that column, it's okay.
            let _ = column.clear_slot(row_index);
        }
        Ok(())
    }

    /// Adds a history entry immediately and emits the appropriate event.
    fn add_history_entry_immediately(
        &mut self,
        label: String,
        reaper_changes: Vec<BoxedReaperChange>,
    ) {
        self.history.add(label, self.save(), reaper_changes);
        self.emit(ClipMatrixEvent::HistoryChanged);
    }

    /// Returns an iterator over all scene-following columns.
    fn scene_columns(&self) -> impl Iterator<Item = &Column> {
        self.content.columns.iter().filter(|c| c.follows_scene())
    }

    /// Returns a mutable iterator over all scene-following columns.
    fn scene_columns_mut(&mut self) -> impl Iterator<Item = &mut Column> {
        self.content
            .columns
            .iter_mut()
            .filter(|c| c.follows_scene())
    }

    /// Cuts the given slot's clips to the matrix clipboard.
    pub fn cut_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.copy_slot(address)?;
        self.clear_slot(address)?;
        Ok(())
    }

    /// Copies the given slot's clips to the matrix clipboard.
    pub fn copy_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        let clips_in_slot = self.get_slot(address)?.api_clips(self.permanent_project());
        self.content.clipboard.content = Some(MatrixClipboardContent::Slot(clips_in_slot));
        Ok(())
    }

    /// Sets the name of the column and the track.
    ///
    /// If the name is `None`, it resets the column name to the name of the track.
    pub fn set_column_name(&mut self, column_index: usize, name: String) -> ClipEngineResult<()> {
        if let Ok(t) = self.get_column(column_index)?.playback_track() {
            // This will cause the matrix to rename the columns as well (uni-directional flow)
            t.set_name(name);
            Ok(())
        } else {
            // Column isn't associated with a track. Rename the column itself.
            self.undoable("Set column name", move |matrix| {
                let column = matrix.get_column_mut(column_index)?;
                column.set_name(name);
                Ok(vec![])
            })
        }
    }

    /// Sets the playback track of the given column.
    pub fn set_column_playback_track(
        &mut self,
        column_index: usize,
        track_id: Option<&TrackId>,
    ) -> ClipEngineResult<()> {
        self.undoable("Set column playback track", move |matrix| {
            matrix
                .content
                .set_column_playback_track(column_index, track_id)?;
            Ok(vec![])
        })
    }

    fn column_rt_equipment(&self) -> ColumnRtEquipment {
        ColumnRtEquipment {
            chain_equipment: &self.content.chain_equipment,
            recorder_request_sender: &self.content.recorder_request_sender,
            matrix_settings: &self.content.settings,
        }
    }

    /// Pastes the clips stored in the matrix clipboard into the given slot.
    // TODO-high-clip-engine In all copy scenarios, we must take care to create new unique IDs!
    pub fn paste_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        let content = self
            .content
            .clipboard
            .content
            .as_ref()
            .ok_or("clipboard empty")?;
        let MatrixClipboardContent::Slot(clips) = content else {
            return Err("clipboard doesn't contain slot contents");
        };
        let cloned_clips = clips.clone();
        self.undoable("Paste slot", move |matrix| {
            matrix.replace_clips_in_slot(address, cloned_clips, IdMode::AssignNewIds)?;
            let event = SlotChangeEvent::Clips("Added clips to slot");
            matrix.emit(ClipMatrixEvent::slot_changed(address, event));
            Ok(vec![])
        })
    }

    /// Clears the given slot.
    pub fn clear_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.undoable("Clear slot", move |matrix| {
            matrix.clear_slot_internal(address)?;
            let event = SlotChangeEvent::Clips("");
            matrix.emit(ClipMatrixEvent::slot_changed(address, event));
            Ok(vec![])
        })
    }

    fn clear_slot_internal(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.get_column_mut(address.column)?
            .clear_slot(address.row)?;
        Ok(())
    }

    /// Adjusts the section lengths of all clips in the given slot.
    pub fn adjust_slot_section_length(
        &mut self,
        address: ClipSlotAddress,
        factor: f64,
    ) -> ClipEngineResult<()> {
        let kit = self.get_slot_kit(address)?;
        kit.slot.adjust_section_length(factor, kit.sender)
    }

    /// Opens the editor for the given clip.
    pub fn start_editing_clip(&mut self, address: ClipAddress) -> ClipEngineResult<()> {
        self.get_column_mut(address.slot_address.column)?
            .start_editing_clip(address.slot_address.row, address.clip_index)
    }

    /// Closes the editor for the given clip.
    pub fn stop_editing_clip(&mut self, address: ClipAddress) -> ClipEngineResult<()> {
        self.get_column_mut(address.slot_address.column)?
            .stop_editing_clip(address.slot_address.row, address.clip_index)
    }

    /// Returns if the editor for the given slot is open.
    pub fn is_editing_clip(&self, address: ClipAddress) -> bool {
        let Ok(slot) = self.get_slot(address.slot_address) else {
            return false;
        };
        slot.is_editing_clip(address.clip_index)
    }

    /// Replaces the given row with the given clips.
    fn replace_row_with_clips(
        &mut self,
        row_index: usize,
        slot_contents: Vec<SlotContentsWithColumn>,
        id_mode: IdMode,
    ) -> ClipEngineResult<()> {
        for slot_content in slot_contents {
            let column =
                match get_column_mut(&mut self.content.columns, slot_content.column_index).ok() {
                    None => break,
                    Some(c) => c,
                };
            column.fill_slot(
                row_index,
                slot_content.value,
                &self.content.chain_equipment,
                &self.content.recorder_request_sender,
                &self.content.settings,
                FillSlotMode::Replace,
                id_mode,
            )?;
        }
        Ok(())
    }

    fn replace_clips_in_slot(
        &mut self,
        address: ClipSlotAddress,
        api_clips: Vec<api::Clip>,
        id_mode: IdMode,
    ) -> ClipEngineResult<()> {
        let column = get_column_mut(&mut self.content.columns, address.column)?;
        column.fill_slot(
            address.row,
            api_clips,
            &self.content.chain_equipment,
            &self.content.recorder_request_sender,
            &self.content.settings,
            FillSlotMode::Replace,
            id_mode,
        )?;
        Ok(())
    }

    /// Replaces the slot contents with the currently selected REAPER item.
    pub fn replace_slot_clips_with_selected_item(
        &mut self,
        address: ClipSlotAddress,
    ) -> ClipEngineResult<()> {
        self.undoable("Fill slot with selected item", |matrix| {
            let column = get_column_mut(&mut matrix.content.columns, address.column)?;
            column.replace_slot_clips_with_selected_item(
                address.row,
                &matrix.content.chain_equipment,
                &matrix.content.recorder_request_sender,
                &matrix.content.settings,
            )?;
            matrix.emit(ClipMatrixEvent::slot_changed(
                address,
                SlotChangeEvent::Clips(""),
            ));
            Ok(vec![])
        })
    }

    /// Plays the given slot.
    pub fn play_slot(
        &self,
        address: ClipSlotAddress,
        options: ColumnPlaySlotOptions,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.content.columns, address.column)?;
        let args = ColumnPlaySlotArgs {
            slot_index: address.row,
            timeline,
            ref_pos: None,
            options,
        };
        column.play_slot(args);
        Ok(())
    }

    pub fn move_slot_to(
        &mut self,
        source_address: ClipSlotAddress,
        dest_address: ClipSlotAddress,
    ) -> ClipEngineResult<()> {
        if source_address == dest_address {
            return Ok(());
        }
        self.undoable("Move slot", |matrix| {
            if source_address.column == dest_address.column {
                // Special handling. We can easily move within the same column.
                let column = matrix.get_column_mut(source_address.column)?;
                column.move_slot_contents(source_address.row, dest_address.row)?;
                matrix.notify_everything_changed();
                Ok(vec![])
            } else {
                let clips_in_slot = matrix
                    .get_slot(source_address)?
                    .api_clips(matrix.permanent_project());
                matrix.clear_slot_internal(source_address)?;
                matrix.replace_clips_in_slot(dest_address, clips_in_slot, IdMode::KeepIds)?;
                matrix.notify_everything_changed();
                Ok(vec![])
            }
        })
    }

    pub fn copy_slot_to(
        &mut self,
        source_address: ClipSlotAddress,
        dest_address: ClipSlotAddress,
    ) -> ClipEngineResult<()> {
        if source_address == dest_address {
            return Ok(());
        }
        let clips_in_slot = self
            .get_slot(source_address)?
            .api_clips(self.permanent_project());
        self.undoable("Copy slot to", |matrix| {
            matrix.replace_clips_in_slot(dest_address, clips_in_slot, IdMode::AssignNewIds)?;
            let event = SlotChangeEvent::Clips("Copied clips to slot");
            matrix.emit(ClipMatrixEvent::slot_changed(dest_address, event));
            Ok(vec![])
        })?;
        Ok(())
    }

    pub fn move_scene_content_to(
        &mut self,
        source_row_index: usize,
        dest_row_index: usize,
    ) -> ClipEngineResult<()> {
        if source_row_index == dest_row_index {
            return Ok(());
        }
        let clips_in_scene = self.all_clips_in_scene(source_row_index).collect();
        self.undoable("Move scene content to", |matrix| {
            matrix.replace_row_with_clips(dest_row_index, clips_in_scene, IdMode::KeepIds)?;
            matrix.clear_scene_internal(source_row_index)?;
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn reorder_columns(
        &mut self,
        source_column_index: usize,
        dest_column_index: usize,
    ) -> ClipEngineResult<()> {
        if source_column_index == dest_column_index {
            return Ok(());
        }
        self.undoable("Reorder columns", |matrix| {
            let source_column = matrix.get_column(source_column_index)?;
            let dest_column = matrix.get_column(dest_column_index)?;
            let reaper_changes: Vec<BoxedReaperChange> =
                if let Ok(change) = matrix.reorder_column_tracks(source_column, dest_column) {
                    vec![Box::new(change)]
                } else {
                    vec![]
                };
            let source_column = matrix.content.columns.remove(source_column_index);
            matrix
                .content
                .columns
                .insert(dest_column_index, source_column);
            matrix.notify_everything_changed();
            Ok(reaper_changes)
        })
    }

    fn reorder_column_tracks(
        &self,
        source_column: &Column,
        dest_column: &Column,
    ) -> ClipEngineResult<TrackReorderingReaperChange> {
        let source_track = source_column.playback_track()?;
        let dest_track = dest_column.playback_track()?;
        let dest_track_index = dest_track
            .index()
            .ok_or("destination track doesn't have index")?;
        reorder_tracks(source_track, dest_track_index)
    }

    pub fn reorder_rows(
        &mut self,
        source_row_index: usize,
        dest_row_index: usize,
    ) -> ClipEngineResult<()> {
        if source_row_index >= self.content.rows.len() {
            return Err("source row doesn't exist");
        }
        if dest_row_index >= self.content.rows.len() {
            return Err("destination row doesn't exist");
        }
        if source_row_index == dest_row_index {
            return Ok(());
        }
        self.undoable("Reorder rows", |matrix| {
            let source_row = matrix.content.rows.remove(source_row_index);
            matrix.content.rows.insert(dest_row_index, source_row);
            for column in &mut matrix.content.columns {
                column.reorder_slots(source_row_index, dest_row_index)?;
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    pub fn copy_scene_content_to(
        &mut self,
        source_row_index: usize,
        dest_row_index: usize,
    ) -> ClipEngineResult<()> {
        if source_row_index == dest_row_index {
            return Ok(());
        }
        let clips_in_scene = self.all_clips_in_scene(source_row_index).collect();
        self.undoable("Copy scene content to", |matrix| {
            matrix.replace_row_with_clips(dest_row_index, clips_in_scene, IdMode::AssignNewIds)?;
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    /// Stops the given slot.
    pub fn stop_slot(
        &self,
        address: ClipSlotAddress,
        stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.content.columns, address.column)?;
        let args = ColumnStopSlotArgs {
            slot_index: address.row,
            timeline,
            ref_pos: None,
            stop_timing,
        };
        column.stop_slot(args);
        Ok(())
    }

    /// Stops all slots in all columns.
    pub fn stop(&self) {
        let timeline = self.timeline();
        let args = ColumnStopArgs {
            ref_pos: Some(timeline.cursor_pos()),
            timeline,
            stop_timing: None,
        };
        for c in &self.content.columns {
            c.stop(args.clone());
        }
    }

    /// Stops all slots in all columns immediately.
    pub fn panic(&self) {
        for c in &self.content.columns {
            c.panic();
        }
    }

    /// Stops column immediately.
    pub fn panic_column(&self, column_index: usize) -> ClipEngineResult<()> {
        self.get_column(column_index)?.panic();
        Ok(())
    }

    /// Stops slot immediately.
    pub fn panic_slot(&self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.get_column(address.column)?.panic_slot(address.row);
        Ok(())
    }

    /// Stops row immediately.
    pub fn panic_row(&self, row_index: usize) -> ClipEngineResult<()> {
        for col in &self.content.columns {
            col.panic_slot(row_index);
        }
        Ok(())
    }

    /// Plays all slots of scene-following columns in the given row.
    pub fn play_scene(&self, index: usize) {
        let timeline = self.timeline();
        let timeline_cursor_pos = timeline.cursor_pos();
        let args = ColumnPlayRowArgs {
            slot_index: index,
            timeline,
            ref_pos: timeline_cursor_pos,
        };
        for c in &self.content.columns {
            c.play_scene(args.clone());
        }
    }

    /// Returns the basic settings of this matrix.
    pub fn settings(&self) -> &MatrixSettings {
        &self.content.settings
    }

    pub fn all_matrix_settings_combined(&self) -> api::MatrixSettings {
        api::MatrixSettings {
            clip_play_settings: self.save_play_settings(),
            clip_record_settings: self.content.settings.clip_record_settings,
            common_tempo_range: self.content.settings.common_tempo_range,
        }
    }

    /// Sets the record duration for new clip recordings.
    pub fn set_record_duration(&mut self, record_length: RecordLength) {
        self.content.settings.clip_record_settings.duration = record_length;
        self.emit(ClipMatrixEvent::RecordDurationChanged);
    }

    /// Builds a scene of all currently playing clips, in the first empty row.
    pub fn build_scene_in_first_empty_row(&mut self) -> ClipEngineResult<()> {
        let empty_row_index = (0usize..)
            .find(|row_index| self.scene_is_empty(*row_index))
            .expect("there's always an empty row");
        self.build_scene_internal(empty_row_index)
    }

    /// Builds a scene of all currently playing clips, in the given row.
    pub fn build_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        if !self.scene_is_empty(row_index) {
            return Err("row is not empty");
        }
        self.build_scene_internal(row_index)
    }

    fn build_scene_internal(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Build scene", |matrix| {
            let playing_clips = matrix.capture_playing_clips();
            matrix.distribute_clips_to_scene(playing_clips, row_index)?;
            matrix.notify_everything_changed();
            Ok(vec![])
        })
    }

    fn notify_everything_changed(&self) {
        self.content.notify_everything_changed();
    }

    fn emit(&self, event: ClipMatrixEvent) {
        self.content.emit(event);
    }

    fn distribute_clips_to_scene(
        &mut self,
        slot_contents: Vec<SlotContentsWithColumn>,
        row_index: usize,
    ) -> ClipEngineResult<()> {
        let need_handle_sync = false;
        for slot_content in slot_contents {
            // First try to put it within same column as clip itself
            let original_column =
                get_column_mut(&mut self.content.columns, slot_content.column_index)?;
            let dest_column =
                if original_column.follows_scene() && original_column.slot_is_empty(row_index) {
                    // We have space in that column, good.
                    original_column
                } else {
                    // We need to find another appropriate column.
                    // TODO-high We shouldn't do that but use the multi-clip-per-slot feature!
                    // let original_column_track = original_column.playback_track().ok().cloned();
                    // let existing_column = self.columns.iter_mut().find(|c| {
                    //     c.follows_scene()
                    //         && c.slot_is_empty(row_index)
                    //         && c.playback_track().ok() == original_column_track.as_ref()
                    // });
                    // if let Some(c) = existing_column {
                    //     // Found.
                    //     c
                    // } else {
                    //     // Not found. Create a new one.
                    //     let same_column = self.columns.get(slot_content.column_index).unwrap();
                    //     let mut duplicate = same_column.duplicate_without_contents();
                    //     duplicate.set_play_mode(ColumnPlayMode::ExclusiveFollowingScene);
                    //     duplicate.sync_matrix_and_column_settings_to_rt_column(&self.settings);
                    //     self.columns.push(duplicate);
                    //     need_handle_sync = true;
                    //     self.columns.last_mut().unwrap()
                    // }
                    todo!()
                };
            dest_column.fill_slot(
                row_index,
                slot_content.value,
                &self.content.chain_equipment,
                &self.content.recorder_request_sender,
                &self.content.settings,
                FillSlotMode::Replace,
                IdMode::AssignNewIds,
            )?;
        }
        if need_handle_sync {
            self.content.sync_column_handles_to_rt_matrix();
        }
        Ok(())
    }

    /// Returns whether the given slot is empty.
    pub fn slot_is_empty(&self, address: ClipSlotAddress) -> bool {
        let Some(column) = self.content.columns.get(address.column) else {
            return false;
        };
        column.slot_is_empty(address.row)
    }

    /// Returns whether the scene of the given row is empty.
    pub fn scene_is_empty(&self, row_index: usize) -> bool {
        self.scene_columns().all(|c| c.slot_is_empty(row_index))
    }

    fn capture_playing_clips(&self) -> Vec<SlotContentsWithColumn> {
        let _project = self.permanent_project();
        self.content
            .columns
            .iter()
            .enumerate()
            .map(|(col_index, col)| {
                let api_clips = col
                    .playing_clips()
                    .filter_map(move |(_, clip)| clip.save().ok());
                SlotContentsWithColumn::new(col_index, api_clips.collect())
            })
            .collect()
    }

    /// Stops all slots in the given column.
    pub fn stop_column(
        &self,
        index: usize,
        stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = self.get_column(index)?;
        let args = ColumnStopArgs {
            timeline,
            ref_pos: None,
            stop_timing,
        };
        column.stop(args);
        Ok(())
    }

    /// Returns a clip timeline for this matrix.
    pub fn timeline(&self) -> HybridTimeline {
        clip_timeline(self.permanent_project(), false)
    }

    fn process_commands(&mut self) {
        while let Ok(task) = self.content.command_receiver.try_recv() {
            match task {
                MatrixCommand::ThrowAway(_) => {}
            }
        }
    }

    fn remove_obsolete_retired_columns(&mut self) {
        self.content.retired_columns.retain(|c| c.is_still_alive());
    }

    /// Polls this matrix and returns a list of gathered events.
    ///
    /// Polling is absolutely essential, e.g. to detect changes or finish recordings.
    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<ClipMatrixEvent> {
        self.poll_history();
        self.remove_obsolete_retired_columns();
        self.process_commands();
        let events: Vec<_> = self
            .content
            .columns
            .iter_mut()
            .enumerate()
            .flat_map(|(column_index, column)| {
                column
                    .poll(timeline_tempo)
                    .into_iter()
                    .map(move |(row_index, event)| {
                        ClipMatrixEvent::slot_changed(
                            ClipSlotAddress::new(column_index, row_index),
                            event,
                        )
                    })
            })
            .collect();
        let undo_point_label = events.iter().find_map(|evt| evt.undo_point_for_polling());
        if let Some(l) = undo_point_label {
            // TODO-high-clip-engine Not sure if we should also add this buffered?
            self.add_history_entry_immediately(l.into(), vec![]);
        }
        // Do occasional checks (roughly two times a second)
        if self.content.poll_count % 15 == 0 {
            self.apply_edited_contents_if_necessary();
        }
        self.content.poll_count += 1;
        // Return polled events
        events
    }

    fn apply_edited_contents_if_necessary(&mut self) {
        for column in &mut self.content.columns {
            column.apply_edited_contents_if_necessary(
                &self.content.chain_equipment,
                &self.content.recorder_request_sender,
                &self.content.settings.overridable,
            );
        }
    }

    pub fn process_reaper_change_events(&mut self, events: &[ChangeEvent]) {
        for event in events {
            match event {
                ChangeEvent::TrackRemoved(e) => {
                    self.remove_all_columns_associated_with_track(&e.track);
                }
                ChangeEvent::TrackNameChanged(e) => {
                    self.rename_all_columns_associated_with_track(&e.track)
                }
                _ => {}
            }
        }
    }

    fn remove_all_columns_associated_with_track(&mut self, track: &Track) {
        let _ = self.undoable("Remove columns due to track removal", |matrix| {
            let mut removed_track = false;
            matrix.content.columns.retain(|col| {
                let keep = !col.uses_track(track);
                if !keep {
                    removed_track = true;
                }
                keep
            });
            if !removed_track {
                // This can happen when the track removal was caused by the matrix itself.
                // In this case, we don't want to create yet another undo point.
                return Err("track removed already");
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        });
    }

    fn rename_all_columns_associated_with_track(&mut self, track: &Track) {
        let _ = self.undoable("Rename columns due to track renaming", |matrix| {
            let track_name = track.name().ok_or("track has no name")?.into_string();
            for col in matrix
                .content
                .columns
                .iter_mut()
                .filter(|c| c.uses_track(track))
            {
                col.set_name(track_name.clone());
            }
            matrix.notify_everything_changed();
            Ok(vec![])
        });
    }

    fn poll_history(&mut self) {
        if !self.history.its_flush_time() {
            return;
        }
        self.history.flush_buffer(self.save());
        self.emit(ClipMatrixEvent::HistoryChanged);
    }

    /// Toggles the loop setting of the given slot.
    pub fn toggle_looped(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.undoable("Toggle looped", |matrix| {
            let kit = matrix.get_slot_kit(address)?;
            let event = kit.slot.toggle_looped(kit.sender)?;
            matrix.emit(ClipMatrixEvent::clip_changed(
                ClipAddress::legacy(address),
                event,
            ));
            Ok(vec![])
        })
    }

    /// Returns whether some slots in this matrix are currently playing/recording.
    pub fn is_stoppable(&self) -> bool {
        self.content.columns.iter().any(|c| c.is_stoppable())
    }

    /// Returns whether the given column is currently playing/recording.
    pub fn column_is_stoppable(&self, index: usize) -> bool {
        self.content
            .columns
            .get(index)
            .map(|c| c.is_stoppable())
            .unwrap_or(false)
    }

    /// Returns whether the given column is armed for recording.
    pub fn column_is_armed_for_recording(&self, index: usize) -> bool {
        self.content
            .columns
            .get(index)
            .map(|c| c.is_armed_for_recording())
            .unwrap_or(false)
    }

    /// Returns if the given track is a playback track in one of the matrix columns.
    pub fn uses_playback_track(&self, track: &Track) -> bool {
        self.content
            .columns
            .iter()
            .any(|c| c.playback_track() == Ok(track))
    }

    /// Returns the number of columns in this matrix.
    pub fn column_count(&self) -> usize {
        self.content.columns.len()
    }

    /// Returns the number of rows in this matrix.
    pub fn row_count(&self) -> usize {
        self.content.rows.len()
    }

    /// Starts MIDI overdubbing the given clip.
    pub fn midi_overdub_clip(&mut self, address: ClipAddress) -> ClipEngineResult<()> {
        let args = EssentialColumnRecordClipArgs {
            matrix_record_settings: &self.content.settings.clip_record_settings,
            chain_equipment: &self.content.chain_equipment,
            recorder_request_sender: &self.content.recorder_request_sender,
            handler: &*self.content.handler,
            containing_track: self.content.containing_track.as_ref(),
            overridable_matrix_settings: &self.content.settings.overridable,
        };
        get_column_mut(&mut self.content.columns, address.slot_address.column)?.midi_overdub_clip(
            address.slot_address.row,
            address.clip_index,
            args,
        )
    }

    /// Starts recording in the given slot.
    pub fn record_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        if self.is_recording() {
            return Err("recording already");
        }
        let args = EssentialColumnRecordClipArgs {
            matrix_record_settings: &self.content.settings.clip_record_settings,
            chain_equipment: &self.content.chain_equipment,
            recorder_request_sender: &self.content.recorder_request_sender,
            handler: &*self.content.handler,
            containing_track: self.content.containing_track.as_ref(),
            overridable_matrix_settings: &self.content.settings.overridable,
        };
        get_column_mut(&mut self.content.columns, address.column())?
            .record_slot(address.row(), args)
    }

    /// Returns whether any column in this matrix is recording.
    pub fn is_recording(&self) -> bool {
        self.content.columns.iter().any(|c| c.is_recording())
    }

    /// Pauses all clips in the given slot.
    pub fn pause_clip(&self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        get_column(&self.content.columns, address.column())?.pause_slot(address.row());
        Ok(())
    }

    /// Seeks the given slot.
    pub fn seek_slot(&self, address: ClipSlotAddress, position: UnitValue) -> ClipEngineResult<()> {
        get_column(&self.content.columns, address.column())?.seek_slot(address.row(), position);
        Ok(())
    }

    /// Sets the volume of the given slot.
    pub fn set_slot_volume(
        &mut self,
        address: ClipSlotAddress,
        volume: Db,
    ) -> ClipEngineResult<()> {
        let kit = self.get_slot_kit(address)?;
        let event = kit.slot.set_volume(volume, kit.sender)?;
        self.emit(ClipMatrixEvent::clip_changed(
            ClipAddress::legacy(address),
            event,
        ));
        Ok(())
    }

    /// Sets the name of the given clip.
    pub fn set_clip_name(
        &mut self,
        address: ClipAddress,
        name: Option<String>,
    ) -> ClipEngineResult<()> {
        self.undoable("Set clip name", |matrix| {
            matrix.get_clip_mut(address)?.set_name(name);
            matrix.emit(ClipMatrixEvent::clip_changed(
                address,
                ClipChangeEvent::Everything,
            ));
            Ok(vec![])
        })
    }

    pub fn set_settings(&mut self, settings: api::MatrixSettings) -> ClipEngineResult<()> {
        self.undoable("Change matrix settings", |matrix| {
            matrix.content.settings.overridable =
                OverridableMatrixSettings::from_api(&settings.clip_play_settings);
            matrix.content.settings.clip_record_settings = settings.clip_record_settings;
            matrix.content.settings.common_tempo_range = settings.common_tempo_range;
            for column in &mut matrix.content.columns {
                column.sync_matrix_settings_to_rt_column(&matrix.content.settings);
            }
            matrix.emit(ClipMatrixEvent::MatrixSettingsChanged);
            Ok(vec![])
        })
    }

    pub fn set_column_settings(
        &mut self,
        column_index: usize,
        settings: api::ColumnSettings,
    ) -> ClipEngineResult<()> {
        self.undoable("Change column settings", |matrix| {
            let column = matrix.get_column_mut(column_index)?;
            column.set_settings(settings);
            matrix.emit(ClipMatrixEvent::ColumnSettingsChanged(column_index));
            Ok(vec![])
        })
    }

    pub fn set_row_data(&mut self, row_index: usize, api_row: api::Row) -> ClipEngineResult<()> {
        self.undoable("Change row data", |matrix| {
            let row = matrix.get_row_mut(row_index)?;
            *row = Row::from_api_row(api_row);
            matrix.emit(ClipMatrixEvent::RowChanged(row_index));
            Ok(vec![])
        })
    }

    /// Applies most properties of the given clip to the clip at the given address.
    ///
    /// The following clip properties will not be changed:
    ///
    /// - ID
    /// - Source(s)
    pub fn set_clip_data(
        &mut self,
        address: ClipAddress,
        api_clip: api::Clip,
    ) -> ClipEngineResult<()> {
        self.undoable("Change clip data", |matrix| {
            let column = matrix.get_column_mut(address.slot_address.column)?;
            column.set_clip_data(address.slot_address.row, address.clip_index, api_clip)?;
            matrix.emit(ClipMatrixEvent::clip_changed(
                address,
                ClipChangeEvent::Everything,
            ));
            Ok(vec![])
        })
    }

    /// Returns the clip at the given address.
    pub fn get_clip(&self, address: ClipAddress) -> ClipEngineResult<&Clip> {
        self.get_slot(address.slot_address)?
            .get_clip(address.clip_index)
    }

    /// Returns the slot at the given address.
    pub fn get_slot(&self, address: ClipSlotAddress) -> ClipEngineResult<&Slot> {
        self.get_column(address.column)?.get_slot(address.row)
    }

    fn get_slot_kit(&mut self, address: ClipSlotAddress) -> ClipEngineResult<SlotKit> {
        self.get_column_mut(address.column)?
            .get_slot_kit_mut(address.row)
    }

    fn get_slot_mut(&mut self, address: ClipSlotAddress) -> ClipEngineResult<&mut Slot> {
        self.get_column_mut(address.column)?
            .get_slot_mut(address.row)
    }

    /// Returns the column at the given index.
    pub fn get_column(&self, index: usize) -> ClipEngineResult<&Column> {
        get_column(&self.content.columns, index)
    }

    /// Returns the row at the given index.
    pub fn get_row(&self, index: usize) -> ClipEngineResult<&Row> {
        get_row(&self.content.rows, index)
    }

    fn get_row_mut(&mut self, index: usize) -> ClipEngineResult<&mut Row> {
        get_row_mut(&mut self.content.rows, index)
    }

    fn get_column_mut(&mut self, index: usize) -> ClipEngineResult<&mut Column> {
        get_column_mut(&mut self.content.columns, index)
    }

    fn get_clip_mut(&mut self, address: ClipAddress) -> ClipEngineResult<&mut Clip> {
        self.get_slot_mut(address.slot_address)?
            .get_clip_mut(address.clip_index)
    }
}

fn get_column(columns: &[Column], index: usize) -> ClipEngineResult<&Column> {
    columns.get(index).ok_or(NO_SUCH_COLUMN)
}

fn get_row(rows: &[Row], index: usize) -> ClipEngineResult<&Row> {
    rows.get(index).ok_or(NO_SUCH_ROW)
}

fn get_row_mut(rows: &mut [Row], index: usize) -> ClipEngineResult<&mut Row> {
    rows.get_mut(index).ok_or(NO_SUCH_ROW)
}

fn get_column_mut(columns: &mut [Column], index: usize) -> ClipEngineResult<&mut Column> {
    columns.get_mut(index).ok_or(NO_SUCH_COLUMN)
}

const NO_SUCH_COLUMN: &str = "no such column";
const NO_SUCH_ROW: &str = "no such row";

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ClipSlotAddress {
    pub column: usize,
    pub row: usize,
}

impl ClipSlotAddress {
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

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ClipAddress {
    pub slot_address: ClipSlotAddress,
    pub clip_index: usize,
}

impl ClipAddress {
    pub fn new(slot_address: ClipSlotAddress, clip_index: usize) -> Self {
        Self {
            slot_address,
            clip_index,
        }
    }

    pub fn legacy(slot_address: ClipSlotAddress) -> Self {
        ClipAddress {
            slot_address,
            clip_index: 0,
        }
    }
}

#[derive(Debug)]
pub struct ClipRecordTask {
    pub input: ClipRecordInput,
    pub destination: ClipRecordDestination,
}

#[derive(Debug)]
pub enum SpecificClipRecordTask {
    HardwareInput(HardwareInputClipRecordTask),
    FxInput(FxInputClipRecordTask),
}

impl ClipRecordTask {
    pub fn create_specific_task(self) -> SpecificClipRecordTask {
        match self.input {
            ClipRecordInput::HardwareInput(input) => {
                let hw_task = HardwareInputClipRecordTask {
                    input,
                    destination: self.destination,
                };
                SpecificClipRecordTask::HardwareInput(hw_task)
            }
            ClipRecordInput::FxInput(input) => {
                let fx_task = FxInputClipRecordTask {
                    input,
                    destination: self.destination,
                };
                SpecificClipRecordTask::FxInput(fx_task)
            }
        }
    }
}

#[derive(Debug)]
pub struct ClipRecordDestination {
    pub column_source: WeakRtColumn,
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
                    // TODO-high-clip-engine Use project quantization settings
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
    MatrixSettingsChanged,
    ColumnSettingsChanged(usize),
    RowChanged(usize),
    EverythingChanged,
    RecordDurationChanged,
    HistoryChanged,
    SlotChanged(QualifiedSlotChangeEvent),
    ClipChanged(QualifiedClipChangeEvent),
}

impl ClipMatrixEvent {
    pub fn slot_changed(slot_address: ClipSlotAddress, event: SlotChangeEvent) -> Self {
        Self::SlotChanged(QualifiedSlotChangeEvent {
            slot_address,
            event,
        })
    }

    pub fn clip_changed(clip_address: ClipAddress, event: ClipChangeEvent) -> Self {
        Self::ClipChanged(QualifiedClipChangeEvent {
            clip_address,
            event,
        })
    }

    pub fn undo_point_for_polling(&self) -> Option<&'static str> {
        match self {
            ClipMatrixEvent::SlotChanged(QualifiedSlotChangeEvent {
                event: SlotChangeEvent::Clips(desc),
                ..
            }) => Some(desc),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ClipRecordTiming {
    StartImmediatelyStopOnDemand,
    StartOnBarStopOnDemand { start_bar: i32 },
    StartOnBarStopOnBar { start_bar: i32, bar_count: u32 },
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
pub struct WithColumn<T> {
    column_index: usize,
    value: T,
}

impl<T> WithColumn<T> {
    fn new(column_index: usize, value: T) -> Self {
        Self {
            column_index,
            value,
        }
    }

    pub fn column_index(&self) -> usize {
        self.column_index
    }

    pub fn value(&self) -> &T {
        &self.value
    }
}

pub type SlotWithColumn<'a> = WithColumn<&'a Slot>;

pub type SlotContentsWithColumn = WithColumn<Vec<api::Clip>>;

fn validate_api_matrix(matrix: &api::Matrix) -> Result<(), ValidationError> {
    ensure_no_duplicate(
        "column IDs",
        matrix.columns.iter().flatten().map(|col| &col.id),
    )?;
    ensure_no_duplicate("row IDs", matrix.rows.iter().flatten().map(|row| &row.id))?;
    ensure_no_duplicate(
        "slot IDs",
        matrix
            .columns
            .iter()
            .flatten()
            .flat_map(|col| col.slots.iter().flatten())
            .map(|slot| &slot.id),
    )?;
    ensure_no_duplicate(
        "clip IDs",
        matrix
            .columns
            .iter()
            .flatten()
            .flat_map(|col| col.slots.iter().flatten())
            .flat_map(|slot| slot.clips.iter().flatten())
            .map(|clip| &clip.id),
    )?;
    Ok(())
}
