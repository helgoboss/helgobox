use crate::base::history::History;
use crate::base::row::Row;
use crate::base::{Clip, Column, Slot, SlotKit};
use crate::rt::supplier::{
    keep_processing_cache_requests, keep_processing_pre_buffer_requests,
    keep_processing_recorder_requests, AudioRecordingEquipment, ChainEquipment,
    ChainPreBufferCommandProcessor, MidiRecordingEquipment, QuantizationSettings, RecorderRequest,
    RecordingEquipment,
};
use crate::rt::{
    ClipChangeEvent, ColumnHandle, ColumnPlayRowArgs, ColumnPlaySlotArgs, ColumnPlaySlotOptions,
    ColumnStopArgs, ColumnStopSlotArgs, FillClipMode, OverridableMatrixSettings,
    QualifiedClipChangeEvent, QualifiedSlotChangeEvent, RtMatrixCommandSender, SlotChangeEvent,
    WeakColumn,
};
use crate::timeline::clip_timeline;
use crate::{rt, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::{Receiver, Sender};
use helgoboss_learn::UnitValue;
use helgoboss_midi::Channel;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    ChannelRange, ClipPlayStartTiming, ClipPlayStopTiming, ColumnPlayMode, Db,
    MatrixClipPlayAudioSettings, MatrixClipPlaySettings, MatrixClipRecordSettings, RecordLength,
    TempoRange,
};
use reaper_high::{OrCurrentProject, Project, Reaper, Track};
use reaper_medium::{Bpm, MidiInputDeviceId};
use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use std::{cmp, mem, thread};

#[derive(Debug)]
pub struct Matrix<H> {
    /// Don't lock this from the main thread, only from real-time threads!
    rt_matrix: rt::SharedMatrix,
    settings: MatrixSettings,
    handler: H,
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
    rt_command_sender: Sender<rt::MatrixCommand>,
    history: History,
    clipboard: MatrixClipboard,
    // We use this just for RAII (joining worker threads when dropped)
    _worker_pool: WorkerPool,
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
    Scene(Vec<ApiClipWithColumn>),
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
            overridable: OverridableMatrixSettings {
                clip_play_start_timing: matrix.clip_play_settings.start_timing,
                clip_play_stop_timing: matrix.clip_play_settings.stop_timing,
                audio_time_stretch_mode: matrix.clip_play_settings.audio_settings.time_stretch_mode,
                audio_resample_mode: matrix.clip_play_settings.audio_settings.resample_mode,
                audio_cache_behavior: matrix.clip_play_settings.audio_settings.cache_behavior,
            },
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

impl<H> Matrix<H> {
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

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Returns the project which contains this matrix, unless the matrix is project-less
    /// (monitoring FX chain).
    pub fn permanent_project(&self) -> Option<Project> {
        self.containing_track.as_ref().map(|t| t.project())
    }

    /// Returns the permanent project. If the matrix is project-less, the current project.
    pub fn temporary_project(&self) -> Project {
        self.permanent_project().or_current_project()
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
            rt_matrix: {
                let m = rt::SharedMatrix::new(rt_matrix);
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
            history: History::new(Default::default()),
            clipboard: Default::default(),
            _worker_pool: worker_pool,
        }
    }

    pub fn real_time_matrix(&self) -> rt::WeakMatrix {
        self.rt_matrix.downgrade()
    }

    pub fn load(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
        self.load_internal(api_matrix)?;
        self.history = History::new(self.save());
        Ok(())
    }

    fn load_internal(&mut self, api_matrix: api::Matrix) -> ClipEngineResult<()> {
        let permanent_project = self.permanent_project();
        self.settings = MatrixSettings::from_api(&api_matrix);
        let mut old_columns: HashMap<_, _> = mem::take(&mut self.columns)
            .into_iter()
            .map(|c| (c.id().clone(), c))
            .collect();
        for api_column in api_matrix.columns.unwrap_or_default().into_iter() {
            let mut column = old_columns
                .remove(&api_column.id)
                .unwrap_or_else(|| Column::new(api_column.id.clone(), permanent_project));
            column.load(
                api_column,
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
            )?;
            self.columns.push(column);
        }
        self.rows = api_matrix
            .rows
            .unwrap_or_default()
            .into_iter()
            .map(|_| Row {})
            .collect();
        self.notify_everything_changed();
        self.sync_column_handles_to_rt();
        // Retire old and now unused columns
        self.retired_columns
            .extend(old_columns.into_values().map(RetiredColumn::new));
        Ok(())
    }

    fn sync_column_handles_to_rt(&mut self) {
        let column_handles = self.columns.iter().map(|c| c.create_handle()).collect();
        self.rt_command_sender.set_column_handles(column_handles);
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
        // TODO-medium-clip-engine We could probably make it work without clone.
        let api_matrix = self.history.undo()?.clone();
        self.load_internal(api_matrix)?;
        self.emit(ClipMatrixEvent::HistoryChanged);
        Ok(())
    }

    pub fn redo(&mut self) -> ClipEngineResult<()> {
        let api_matrix = self.history.redo()?.clone();
        self.load_internal(api_matrix)?;
        self.emit(ClipMatrixEvent::HistoryChanged);
        Ok(())
    }

    fn undoable<R>(
        &mut self,
        label: impl Into<String>,
        f: impl FnOnce(&mut Self) -> ClipEngineResult<R>,
    ) -> ClipEngineResult<R> {
        let result = f(self);
        if result.is_ok() {
            self.add_history_entry(label.into());
        }
        result
    }

    /// Freezes the complete matrix.
    pub async fn freeze(&mut self) {
        for (i, column) in self.columns.iter_mut().enumerate() {
            let _ = column.freeze(i).await;
        }
    }

    /// Takes the current effective matrix dimensions into account, so even if a slot doesn't exist
    /// yet physically in the column, it returns `true` if it *should* exist.
    pub fn slot_exists(&self, coordinates: ClipSlotAddress) -> bool {
        coordinates.column < self.columns.len() && coordinates.row < self.row_count()
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
        self.columns
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
        self.columns.iter()
    }

    /// Returns an iterator over all clips in a row whose column is a scene follower.
    fn all_clips_in_scene(&self, row_index: usize) -> impl Iterator<Item = ApiClipWithColumn> + '_ {
        let project = self.permanent_project();
        self.columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.follows_scene())
            .filter_map(move |(i, c)| Some((i, c.get_slot(row_index).ok()?)))
            .flat_map(move |(i, s)| {
                s.clips().filter_map(move |clip| {
                    let api_clip = clip.save(project).ok()?;
                    Some(ApiClipWithColumn::new(i, api_clip))
                })
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
        self.clipboard.content = Some(MatrixClipboardContent::Scene(clips));
        Ok(())
    }

    /// Pastes the clips stored in the matrix clipboard into the given scene.
    pub fn paste_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        let content = self.clipboard.content.as_ref().ok_or("clipboard empty")?;
        let MatrixClipboardContent::Scene(clips) = content else {
            return Err("clipboard doesn't contain scene contents");
        };
        let cloned_clips = clips.clone();
        self.undoable("Fill row with clips", |matrix| {
            matrix.replace_row_with_clips(row_index, cloned_clips)?;
            matrix.notify_everything_changed();
            Ok(())
        })
    }

    pub fn remove_column(&mut self, column_index: usize) -> ClipEngineResult<()> {
        if column_index >= self.columns.len() {
            return Err("column doesn't exist");
        }
        self.undoable("Remove column", |matrix| {
            let column = matrix.columns.remove(column_index);
            let timeline = matrix.timeline();
            column.stop(ColumnStopArgs {
                timeline,
                ref_pos: None,
                stop_timing: Some(ClipPlayStopTiming::Immediately),
            });
            matrix.retired_columns.push(RetiredColumn::new(column));
            matrix.sync_column_handles_to_rt();
            matrix.notify_everything_changed();
            Ok(())
        })
    }

    /// Clears the slots of all scene-following columns.
    pub fn clear_scene(&mut self, row_index: usize) -> ClipEngineResult<()> {
        self.undoable("Clear scene", |matrix| {
            matrix.clear_scene_internal(row_index)?;
            matrix.notify_everything_changed();
            Ok(())
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

    /// Adds a history entry and emits the appropriate event.
    fn add_history_entry(&mut self, label: String) {
        self.history.add(label, self.save());
        self.emit(ClipMatrixEvent::HistoryChanged);
    }

    /// Returns an iterator over all scene-following columns.
    fn scene_columns(&self) -> impl Iterator<Item = &Column> {
        self.columns.iter().filter(|c| c.follows_scene())
    }

    /// Returns a mutable iterator over all scene-following columns.
    fn scene_columns_mut(&mut self) -> impl Iterator<Item = &mut Column> {
        self.columns.iter_mut().filter(|c| c.follows_scene())
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
        self.clipboard.content = Some(MatrixClipboardContent::Slot(clips_in_slot));
        Ok(())
    }

    /// Pastes the clips stored in the matrix clipboard into the given slot.
    pub fn paste_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        let content = self.clipboard.content.as_ref().ok_or("clipboard empty")?;
        let MatrixClipboardContent::Slot(clips) = content else {
            return Err("clipboard doesn't contain slot contents");
        };
        let cloned_clips = clips.clone();
        self.undoable("Paste slot", move |matrix| {
            matrix.add_clips_to_slot(address, cloned_clips)?;
            let event = SlotChangeEvent::Clips("Added clips to slot");
            matrix.emit(ClipMatrixEvent::slot_changed(address, event));
            Ok(())
        })
    }

    /// Clears the given slot.
    pub fn clear_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.undoable("Clear slot", move |matrix| {
            matrix.clear_slot_internal(address)?;
            let event = SlotChangeEvent::Clips("");
            matrix.emit(ClipMatrixEvent::slot_changed(address, event));
            Ok(())
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

    /// Opens the editor for the given slot.
    pub fn start_editing_slot(&self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.get_slot(address)?
            .start_editing(self.temporary_project())
    }

    /// Closes the editor for the given slot.
    pub fn stop_editing_slot(&self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        self.get_slot(address)?
            .stop_editing(self.temporary_project())
    }

    /// Returns if the editor for the given slot is open.
    pub fn is_editing_slot(&self, address: ClipSlotAddress) -> bool {
        let Ok(slot) = self.get_slot(address) else {
            return false;
        };
        slot.is_editing_clip(self.temporary_project())
    }

    /// Replaces the given row with the given clips.
    fn replace_row_with_clips(
        &mut self,
        row_index: usize,
        clips: Vec<ApiClipWithColumn>,
    ) -> ClipEngineResult<()> {
        for clip in clips {
            let column = match get_column_mut(&mut self.columns, clip.column_index).ok() {
                None => break,
                Some(c) => c,
            };
            column.fill_slot_with_clip(
                row_index,
                clip.value,
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
                FillClipMode::Replace,
            )?;
        }
        Ok(())
    }

    /// Adds the given clips to the given slot.
    fn add_clips_to_slot(
        &mut self,
        address: ClipSlotAddress,
        api_clips: Vec<api::Clip>,
    ) -> ClipEngineResult<()> {
        let column = get_column_mut(&mut self.columns, address.column)?;
        for api_clip in api_clips {
            // TODO-high-clip-engine CONTINUE Starting from here, don't let the methods return events anymore!
            //  Mmh, or maybe not. The deep method can know better what changed (e.g. toggle_looped
            //  for all clips). But on the other hand, it doesn't know about batches
            //  and might therefore build a list of events for nothing!
            column.fill_slot_with_clip(
                address.row,
                api_clip,
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
                FillClipMode::Add,
            )?;
        }
        Ok(())
    }

    /// Replaces the slot contents with the currently selected REAPER item.
    pub fn replace_slot_contents_with_selected_item(
        &mut self,
        address: ClipSlotAddress,
    ) -> ClipEngineResult<()> {
        self.undoable("Fill slot with selected item", |matrix| {
            let column = get_column_mut(&mut matrix.columns, address.column)?;
            let event = column.replace_slot_contents_with_selected_item(
                address.row,
                &matrix.chain_equipment,
                &matrix.recorder_request_sender,
                &matrix.settings,
            )?;
            matrix.emit(ClipMatrixEvent::slot_changed(address, event));
            Ok(())
        })
    }

    /// Plays the given slot.
    pub fn play_slot(
        &self,
        address: ClipSlotAddress,
        options: ColumnPlaySlotOptions,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, address.column)?;
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
        let clips_in_slot = self
            .get_slot(source_address)?
            .api_clips(self.permanent_project());
        self.undoable("Move slot to", |matrix| {
            matrix.clear_slot_internal(source_address)?;
            matrix.add_clips_to_slot(dest_address, clips_in_slot)?;
            matrix.notify_everything_changed();
            Ok(())
        })?;
        Ok(())
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
            matrix.add_clips_to_slot(dest_address, clips_in_slot)?;
            let event = SlotChangeEvent::Clips("Copied clips to slot");
            matrix.emit(ClipMatrixEvent::slot_changed(dest_address, event));
            Ok(())
        })?;
        Ok(())
    }

    pub fn move_scene_to(
        &mut self,
        source_row_index: usize,
        dest_row_index: usize,
    ) -> ClipEngineResult<()> {
        if source_row_index == dest_row_index {
            return Ok(());
        }
        let clips_in_scene = self.all_clips_in_scene(source_row_index).collect();
        self.undoable("Move scene to", |matrix| {
            matrix.replace_row_with_clips(dest_row_index, clips_in_scene)?;
            matrix.clear_scene_internal(source_row_index)?;
            matrix.notify_everything_changed();
            Ok(())
        })
    }

    pub fn copy_scene_to(
        &mut self,
        source_row_index: usize,
        dest_row_index: usize,
    ) -> ClipEngineResult<()> {
        if source_row_index == dest_row_index {
            return Ok(());
        }
        let clips_in_scene = self.all_clips_in_scene(source_row_index).collect();
        self.undoable("Copy scene to", |matrix| {
            matrix.replace_row_with_clips(dest_row_index, clips_in_scene)?;
            matrix.notify_everything_changed();
            Ok(())
        })
    }

    /// Plays the given slot.
    pub fn stop_slot(
        &self,
        address: ClipSlotAddress,
        stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipEngineResult<()> {
        let timeline = self.timeline();
        let column = get_column(&self.columns, address.column)?;
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
        for c in &self.columns {
            c.stop(args.clone());
        }
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
        for c in &self.columns {
            c.play_scene(args.clone());
        }
    }

    /// Returns the settings of this matrix.
    pub fn settings(&self) -> &MatrixSettings {
        &self.settings
    }

    /// Sets the record duration for new clip recordings.
    pub fn set_record_duration(&mut self, record_length: RecordLength) {
        self.settings.clip_record_settings.duration = record_length;
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
            let playing_clips = matrix.capture_playing_clips()?;
            matrix.distribute_clips_to_scene(playing_clips, row_index)?;
            matrix.notify_everything_changed();
            Ok(())
        })
    }

    fn notify_everything_changed(&self) {
        self.emit(ClipMatrixEvent::EverythingChanged);
    }

    fn emit(&self, event: ClipMatrixEvent) {
        self.handler.emit_event(event);
    }

    fn distribute_clips_to_scene(
        &mut self,
        clips: Vec<ApiClipWithColumn>,
        row_index: usize,
    ) -> ClipEngineResult<()> {
        let mut need_handle_sync = false;
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
                        duplicate.sync_matrix_settings_to_rt(&self.settings);
                        self.columns.push(duplicate);
                        need_handle_sync = true;
                        self.columns.last_mut().unwrap()
                    }
                };
            dest_column.fill_slot_with_clip(
                row_index,
                clip.value,
                &self.chain_equipment,
                &self.recorder_request_sender,
                &self.settings,
                FillClipMode::Replace,
            )?;
        }
        if need_handle_sync {
            self.sync_column_handles_to_rt();
        }
        Ok(())
    }

    /// Returns whether the given slot is empty.
    pub fn slot_is_empty(&self, address: ClipSlotAddress) -> bool {
        let Some(column) = self.columns.get(address.column) else {
            return false;
        };
        column.slot_is_empty(address.row)
    }

    /// Returns whether the scene of the given row is empty.
    pub fn scene_is_empty(&self, row_index: usize) -> bool {
        self.scene_columns().all(|c| c.slot_is_empty(row_index))
    }

    fn capture_playing_clips(&self) -> ClipEngineResult<Vec<ApiClipWithColumn>> {
        let project = self.permanent_project();
        self.columns
            .iter()
            .enumerate()
            .flat_map(|(col_index, col)| {
                col.playing_clips().map(move |(_, clip)| {
                    let api_clip = clip.save(project)?;
                    Ok(ApiClipWithColumn::new(col_index, api_clip))
                })
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
        while let Ok(task) = self.command_receiver.try_recv() {
            match task {
                MatrixCommand::ThrowAway(_) => {}
            }
        }
    }

    fn remove_obsolete_retired_columns(&mut self) {
        self.retired_columns.retain(|c| c.is_still_alive());
    }

    /// Polls this matrix and returns a list of gathered events.
    ///
    /// Polling is absolutely essential, e.g. to detect changes or finish recordings.
    pub fn poll(&mut self, timeline_tempo: Bpm) -> Vec<ClipMatrixEvent> {
        self.remove_obsolete_retired_columns();
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
                        ClipMatrixEvent::slot_changed(
                            ClipSlotAddress::new(column_index, row_index),
                            event,
                        )
                    })
            })
            .collect();
        let undo_point_label = events.iter().find_map(|evt| evt.undo_point_for_polling());
        if let Some(l) = undo_point_label {
            self.add_history_entry(l.into());
        }
        events
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
            Ok(())
        })
    }

    /// Returns whether some slots in this matrix are currently playing/recording.
    pub fn is_stoppable(&self) -> bool {
        self.columns.iter().any(|c| c.is_stoppable())
    }

    /// Returns whether the given column is currently playing/recording.
    pub fn column_is_stoppable(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_stoppable())
            .unwrap_or(false)
    }

    /// Returns whether the given column is armed for recording.
    pub fn column_is_armed_for_recording(&self, index: usize) -> bool {
        self.columns
            .get(index)
            .map(|c| c.is_armed_for_recording())
            .unwrap_or(false)
    }

    /// Returns if the given track is a playback track in one of the matrix columns.
    pub fn uses_playback_track(&self, track: &Track) -> bool {
        self.columns.iter().any(|c| c.playback_track() == Ok(track))
    }

    /// Returns the number of columns in this matrix.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns the number of rows in this matrix.
    ///
    /// It's possible that the actual number of slots in a column is *higher* than the
    /// official row count of this matrix. In that case, the higher number is returned.
    pub fn row_count(&self) -> usize {
        let max_slot_count_per_col = self
            .columns
            .iter()
            .map(|c| c.slot_count())
            .max()
            .unwrap_or(0);
        cmp::max(self.rows.len(), max_slot_count_per_col)
    }

    /// Starts recording in the given slot.
    pub fn record_slot(&mut self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        if self.is_recording() {
            return Err("recording already");
        }
        get_column_mut(&mut self.columns, address.column())?.record_slot(
            address.row(),
            &self.settings.clip_record_settings,
            &self.chain_equipment,
            &self.recorder_request_sender,
            &self.handler,
            self.containing_track.as_ref(),
            &self.settings.overridable,
        )
    }

    /// Returns whether any column in this matrix is recording.
    pub fn is_recording(&self) -> bool {
        self.columns.iter().any(|c| c.is_recording())
    }

    /// Pauses all clips in the given slot.
    pub fn pause_clip(&self, address: ClipSlotAddress) -> ClipEngineResult<()> {
        get_column(&self.columns, address.column())?.pause_slot(address.row());
        Ok(())
    }

    /// Seeks the given slot.
    pub fn seek_slot(&self, address: ClipSlotAddress, position: UnitValue) -> ClipEngineResult<()> {
        get_column(&self.columns, address.column())?.seek_slot(address.row(), position);
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
        let event = self.get_clip_mut(address)?.set_name(name);
        self.emit(ClipMatrixEvent::clip_changed(address, event));
        Ok(())
    }

    /// Sets the complete data of the given clip.
    pub fn set_clip_data(
        &mut self,
        address: ClipAddress,
        api_clip: api::Clip,
    ) -> ClipEngineResult<()> {
        let clip = self.get_clip_mut(address)?;
        *clip = Clip::load(api_clip);
        // TODO-high-clip-engine Sync important data to real-time processor
        self.emit(ClipMatrixEvent::clip_changed(
            address,
            ClipChangeEvent::Everything,
        ));
        Ok(())
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
        get_column(&self.columns, index)
    }

    fn get_column_mut(&mut self, index: usize) -> ClipEngineResult<&mut Column> {
        get_column_mut(&mut self.columns, index)
    }

    fn get_clip_mut(&mut self, address: ClipAddress) -> ClipEngineResult<&mut Clip> {
        self.get_slot_mut(address.slot_address)?
            .get_clip_mut(address.clip_index)
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

pub trait ClipMatrixHandler: Sized {
    fn request_recording_input(&self, task: ClipRecordTask);
    fn emit_event(&self, event: ClipMatrixEvent);
}

#[derive(Debug)]
pub enum ClipMatrixEvent {
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
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

pub type ApiClipWithColumn = WithColumn<api::Clip>;
