use crate::main::{ClipSlotCoordinates, MainMatrixCommandSender};
use crate::rt::{
    ColumnPlayClipArgs, ColumnStopClipArgs, RelevantPlayStateChange, SharedColumn,
    SlotProcessTransportChangeArgs, TransportChange, WeakColumn,
};
use crate::{clip_timeline, main, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::{Receiver, Sender};
use playtime_api::{ClipPlayStartTiming, ClipPlayStopTiming};
use reaper_high::{Project, Reaper};
use reaper_medium::{PlayState, ProjectContext, ReaperPointer};
use std::borrow::BorrowMut;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// The real-time matrix is supposed to be used from real-time threads.
///
/// It locks the column sources because it can be sure that there's no contention. Well, at least
/// if "Live FX multi-processing" is disabled - because then both the lock code and the preview
/// register playing are driven by the same (audio interface) thread and one thread can't do
/// multiple things in parallel.
///
/// If "Live FX multi-processing" is enabled, the preview register playing will be driven by
/// different worker threads. Still, according to Justin, the locking should be okay even then. As
/// long as the worker threads don't hold the lock for too long. Which probably means that there's
/// no high risk of priority inversion because the worker threads have a higher priority as well.
///
/// In theory, we could replace locking in favor of channels even here. But there's one significant
/// catch: It means the calling code can't "look into" the matrix and query current state before
/// executing a play, stop or whatever ... a sender is only uni-directional (bi-directional async
/// communication would also be possible but it's more complex to get it right in particular with
/// multiple similar requests coming in in sequence that depend on the result of the previous
/// request). So simply toggling play/stop would already need a channel message (= ReaLearn target)
/// that does just this. Which would be a deviation of ReaLearn's concept where the "glue" section
/// can decide what to do based on the current target value. But in case we need it, that would be
/// the way to go.
#[derive(Debug)]
pub struct Matrix {
    settings: MatrixSettings,
    columns: Vec<WeakColumn>,
    command_receiver: Receiver<MatrixCommand>,
    main_command_sender: Sender<main::MatrixCommand>,
    project: Option<Project>,
    last_project_play_state: PlayState,
    play_position_jump_detector: PlayPositionJumpDetector,
}

#[derive(Clone, Debug, Default)]
pub struct MatrixSettings {
    pub clip_play_start_timing: ClipPlayStartTiming,
    pub clip_play_stop_timing: ClipPlayStopTiming,
}

#[derive(Clone, Debug)]
pub struct SharedMatrix(Arc<Mutex<Matrix>>);

#[derive(Clone, Debug)]
pub struct WeakMatrix(Weak<Mutex<Matrix>>);

impl SharedMatrix {
    pub fn new(matrix: Matrix) -> Self {
        Self(Arc::new(Mutex::new(matrix)))
    }

    /// The real-time matrix should be locked only from real-time threads.
    ///
    /// Then we have no contention and this is super fast.
    pub fn lock(&self) -> MutexGuard<Matrix> {
        match self.0.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        }
    }

    pub fn downgrade(&self) -> WeakMatrix {
        WeakMatrix(Arc::downgrade(&self.0))
    }
}

impl WeakMatrix {
    pub fn upgrade(&self) -> Option<SharedMatrix> {
        self.0.upgrade().map(SharedMatrix)
    }
}

impl Matrix {
    pub fn new(
        command_receiver: Receiver<MatrixCommand>,
        command_sender: Sender<main::MatrixCommand>,
        project: Option<Project>,
    ) -> Self {
        Self {
            settings: Default::default(),
            // TODO-high Choose capacity wisely
            columns: Vec::with_capacity(500),
            command_receiver,
            main_command_sender: command_sender,
            project,
            last_project_play_state: get_project_play_state(project),
            play_position_jump_detector: PlayPositionJumpDetector::new(project),
        }
    }

    pub fn poll(&mut self) {
        if let Some(p) = self.project {
            if !p.is_available() {
                return;
            }
        }
        let relevant_transport_change_detected = self.detect_and_process_transport_change();
        if !relevant_transport_change_detected {
            self.detect_and_process_play_position_jump();
        }
        while let Ok(command) = self.command_receiver.try_recv() {
            use MatrixCommand::*;
            match command {
                InsertColumn(index, source) => {
                    self.columns.insert(index, source);
                }
                RemoveColumn(index) => {
                    let column = self.columns.remove(index);
                    self.main_command_sender.throw_away(column);
                }
                ClearColumns => {
                    for column in self.columns.drain(..) {
                        self.main_command_sender.throw_away(column);
                    }
                }
                UpdateSettings(s) => {
                    self.settings = s;
                }
            }
        }
    }

    fn detect_and_process_transport_change(&mut self) -> bool {
        let timeline = clip_timeline(self.project, false);
        let new_play_state = get_project_play_state(self.project);
        let last_play_state = mem::replace(&mut self.last_project_play_state, new_play_state);
        if let Some(relevant) =
            RelevantPlayStateChange::from_play_state_change(last_play_state, new_play_state)
        {
            let args = SlotProcessTransportChangeArgs {
                change: TransportChange::PlayState(relevant),
                timeline: &timeline,
                timeline_cursor_pos: timeline.cursor_pos(),
                parent_clip_play_start_timing: self.settings.clip_play_start_timing,
                parent_clip_play_stop_timing: self.settings.clip_play_stop_timing,
            };
            for column in self.columns.iter().filter_map(|c| c.upgrade()) {
                column.lock().process_transport_change(args.clone());
            }
            true
        } else {
            false
        }
    }

    fn detect_and_process_play_position_jump(&mut self) {
        if !self.play_position_jump_detector.detect_play_jump() {
            return;
        }
        let timeline = clip_timeline(self.project, true);
        let args = SlotProcessTransportChangeArgs {
            change: TransportChange::PlayCursorJump,
            timeline: &timeline,
            timeline_cursor_pos: timeline.cursor_pos(),
            parent_clip_play_start_timing: self.settings.clip_play_start_timing,
            parent_clip_play_stop_timing: self.settings.clip_play_stop_timing,
        };
        for column in self.columns.iter().filter_map(|c| c.upgrade()) {
            column.lock().process_transport_change(args.clone());
        }
    }

    pub fn play_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = self.column_internal(coordinates.column())?;
        let args = ColumnPlayClipArgs {
            slot_index: coordinates.row(),
            parent_start_timing: self.settings.clip_play_start_timing,
            parent_stop_timing: self.settings.clip_play_stop_timing,
            // TODO-medium This could be optimized. In real-time context, getting the timeline only
            //  once per block could save some resources. Sample with clip stop.
            timeline: self.timeline(),
            // TODO-medium We could even take the frame offset of the MIDI
            //  event into account and from that calculate the exact timeline position (within the
            //  block). That amount of accuracy is probably not necessary, but it's almost too easy
            //  to implement to not do it ... same with clip stop.
            ref_pos: None,
        };
        column.lock().borrow_mut().play_clip(args)?;
        Ok(())
    }

    pub fn stop_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = self.column_internal(coordinates.column())?;
        let args = ColumnStopClipArgs {
            slot_index: coordinates.row(),
            parent_start_timing: self.settings.clip_play_start_timing,
            parent_stop_timing: self.settings.clip_play_stop_timing,
            timeline: self.timeline(),
            ref_pos: None,
        };
        column.lock().borrow_mut().stop_clip(args)?;
        Ok(())
    }

    fn timeline(&self) -> HybridTimeline {
        clip_timeline(self.project, false)
    }

    pub fn pause_clip(&self, coordinates: ClipSlotCoordinates) -> ClipEngineResult<()> {
        let column = self.column_internal(coordinates.column())?;
        column.lock().borrow_mut().pause_clip(coordinates.row())?;
        Ok(())
    }

    pub fn column(&self, index: usize) -> ClipEngineResult<SharedColumn> {
        self.column_internal(index)
    }

    fn column_internal(&self, index: usize) -> ClipEngineResult<SharedColumn> {
        let column = self.columns.get(index).ok_or("column doesn't exist")?;
        column.upgrade().ok_or("column doesn't exist anymore")
    }
}

pub enum MatrixCommand {
    InsertColumn(usize, WeakColumn),
    RemoveColumn(usize),
    ClearColumns,
    UpdateSettings(MatrixSettings),
}

pub trait RtMatrixCommandSender {
    fn insert_column(&self, index: usize, source: WeakColumn);
    fn remove_column(&self, index: usize);
    fn clear_columns(&self);
    fn update_settings(&self, settings: MatrixSettings);
    fn send_command(&self, command: MatrixCommand);
}

impl RtMatrixCommandSender for Sender<MatrixCommand> {
    fn insert_column(&self, index: usize, source: WeakColumn) {
        self.send_command(MatrixCommand::InsertColumn(index, source));
    }

    fn update_settings(&self, settings: MatrixSettings) {
        self.send_command(MatrixCommand::UpdateSettings(settings))
    }

    fn remove_column(&self, index: usize) {
        self.send_command(MatrixCommand::RemoveColumn(index));
    }

    fn clear_columns(&self) {
        self.send_command(MatrixCommand::ClearColumns);
    }

    fn send_command(&self, command: MatrixCommand) {
        self.try_send(command).unwrap();
    }
}

fn get_project_play_state(project: Option<Project>) -> PlayState {
    let project_context = get_project_context(project);
    Reaper::get()
        .medium_reaper()
        .get_play_state_ex(project_context)
}

fn get_project_context(project: Option<Project>) -> ProjectContext {
    if let Some(p) = project {
        p.context()
    } else {
        ProjectContext::CurrentProject
    }
}

/// Detects play position discontinuity while the project is playing, ignoring tempo changes.
#[derive(Debug)]
struct PlayPositionJumpDetector {
    project_context: ProjectContext,
    previous_beat: Option<isize>,
}

impl PlayPositionJumpDetector {
    pub fn new(project: Option<Project>) -> Self {
        Self {
            project_context: project
                .map(|p| p.context())
                .unwrap_or(ProjectContext::CurrentProject),
            previous_beat: None,
        }
    }

    /// Returns `true` if a jump has been detected.
    ///
    /// To be called in each audio block.
    pub fn detect_play_jump(&mut self) -> bool {
        let reaper = Reaper::get().medium_reaper();
        if let ProjectContext::Proj(p) = self.project_context {
            if !reaper.validate_ptr(ReaperPointer::ReaProject(p)) {
                // Project doesn't exist anymore. Happens when closing it.
                return false;
            }
        }
        let play_state = reaper.get_play_state_ex(self.project_context);
        if !play_state.is_playing {
            return false;
        }
        let play_pos = reaper.get_play_position_2_ex(self.project_context);
        let res = reaper.time_map_2_time_to_beats(self.project_context, play_pos);
        // TODO-high-later If we skip slightly forward within the beat or just to the next beat, the
        //  detector won't detect it as a jump. Destroys the synchronization.
        let beat = res.full_beats.get() as isize;
        if let Some(previous_beat) = self.previous_beat.replace(beat) {
            let beat_diff = beat - previous_beat;
            !(0..=1).contains(&beat_diff)
        } else {
            // Don't count initial change as jump.
            false
        }
    }
}
