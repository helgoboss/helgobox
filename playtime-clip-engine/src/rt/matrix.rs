use crate::base::{ClipSlotAddress, MainMatrixCommandSender, MatrixGarbage};
use crate::mutex_util::non_blocking_lock;
use crate::rt::{
    BasicAudioRequestProps, ColumnCommandSender, ColumnPlayClipOptions, ColumnPlayRowArgs,
    ColumnPlaySlotArgs, ColumnProcessTransportChangeArgs, ColumnStopArgs, ColumnStopSlotArgs,
    RelevantPlayStateChange, SharedColumn, TransportChange, WeakColumn,
};
use crate::{base, clip_timeline, ClipEngineResult, HybridTimeline, Timeline};
use crossbeam_channel::{Receiver, Sender};
use playtime_api::persistence::ClipPlayStopTiming;
use reaper_high::{Project, Reaper};
use reaper_medium::{PlayState, ProjectContext, ReaperPointer};
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

/// A column here is just a weak pointer. 1000 pointers take just around 8 kB in total.
const MAX_COLUMN_COUNT_WITHOUT_REALLOCATION: usize = 1000;

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
    column_handles: Vec<ColumnHandle>,
    command_receiver: Receiver<MatrixCommand>,
    main_command_sender: Sender<base::MatrixCommand>,
    project: Option<Project>,
    last_project_play_state: PlayState,
    play_position_jump_detector: PlayPositionJumpDetector,
}

#[derive(Debug)]
pub struct ColumnHandle {
    pub pointer: WeakColumn,
    pub command_sender: ColumnCommandSender,
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
        non_blocking_lock(&self.0, "real-time matrix")
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
        command_sender: Sender<base::MatrixCommand>,
        project: Option<Project>,
    ) -> Self {
        Self {
            column_handles: Vec::with_capacity(MAX_COLUMN_COUNT_WITHOUT_REALLOCATION),
            command_receiver,
            main_command_sender: command_sender,
            project,
            last_project_play_state: get_project_play_state(project),
            play_position_jump_detector: PlayPositionJumpDetector::new(project),
        }
    }

    pub fn poll(&mut self, audio_request_props: BasicAudioRequestProps) {
        if let Some(p) = self.project {
            if !p.is_available() {
                return;
            }
        }
        let relevant_transport_change_detected =
            self.detect_and_process_transport_change(audio_request_props);
        if !relevant_transport_change_detected {
            self.detect_and_process_play_position_jump(audio_request_props);
        }
        while let Ok(command) = self.command_receiver.try_recv() {
            use MatrixCommand::*;
            match command {
                SetColumnHandles(handles) => {
                    let old_handles = mem::replace(&mut self.column_handles, handles);
                    self.main_command_sender
                        .throw_away(MatrixGarbage::ColumnHandles(old_handles));
                }
            }
        }
    }

    fn detect_and_process_transport_change(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
    ) -> bool {
        let timeline = clip_timeline(self.project, false);
        let new_play_state = get_project_play_state(self.project);
        let last_play_state = mem::replace(&mut self.last_project_play_state, new_play_state);
        if let Some(relevant) =
            RelevantPlayStateChange::from_play_state_change(last_play_state, new_play_state)
        {
            let args = ColumnProcessTransportChangeArgs {
                change: TransportChange::PlayState(relevant),
                timeline: timeline.clone(),
                timeline_cursor_pos: timeline.cursor_pos(),
                audio_request_props,
            };
            for handle in &self.column_handles {
                handle.command_sender.process_transport_change(args.clone());
            }
            true
        } else {
            false
        }
    }

    fn detect_and_process_play_position_jump(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
    ) {
        if !self.play_position_jump_detector.detect_play_jump() {
            return;
        }
        let timeline = clip_timeline(self.project, true);
        let args = ColumnProcessTransportChangeArgs {
            change: TransportChange::PlayCursorJump,
            timeline: timeline.clone(),
            timeline_cursor_pos: timeline.cursor_pos(),
            audio_request_props,
        };
        for handle in &self.column_handles {
            handle.command_sender.process_transport_change(args.clone());
        }
    }

    pub fn play_clip(
        &self,
        coordinates: ClipSlotAddress,
        options: ColumnPlayClipOptions,
    ) -> ClipEngineResult<()> {
        let handle = self.column_handle(coordinates.column())?;
        let args = ColumnPlaySlotArgs {
            slot_index: coordinates.row(),
            // TODO-medium This could be optimized. In real-time context, getting the timeline only
            //  once per block could save some resources. Sample with clip stop.
            timeline: self.timeline(),
            // TODO-medium We could even take the frame offset of the MIDI
            //  event into account and from that calculate the exact timeline position (within the
            //  block). That amount of accuracy is probably not necessary, but it's almost too easy
            //  to implement to not do it ... same with clip stop.
            ref_pos: None,
            options,
        };
        handle.command_sender.play_slot(args);
        Ok(())
    }

    pub fn stop_clip(
        &self,
        coordinates: ClipSlotAddress,
        stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipEngineResult<()> {
        let handle = self.column_handle(coordinates.column())?;
        let args = ColumnStopSlotArgs {
            slot_index: coordinates.row(),
            timeline: self.timeline(),
            ref_pos: None,
            stop_timing,
        };
        handle.command_sender.stop_slot(args);
        Ok(())
    }

    pub fn is_stoppable(&self) -> bool {
        self.columns_internal().any(|h| h.lock().is_stoppable())
    }

    pub fn column_is_stoppable(&self, index: usize) -> bool {
        self.column_internal(index)
            .map(|c| c.lock().is_stoppable())
            .unwrap_or(false)
    }

    pub fn stop(&self) {
        let timeline = self.timeline();
        let args = ColumnStopArgs {
            ref_pos: Some(timeline.cursor_pos()),
            timeline,
        };
        for handle in &self.column_handles {
            handle.command_sender.stop(args.clone());
        }
    }

    pub fn stop_column(&self, index: usize) -> ClipEngineResult<()> {
        let handle = self.column_handle(index)?;
        let args = ColumnStopArgs {
            timeline: self.timeline(),
            ref_pos: None,
        };
        handle.command_sender.stop(args);
        Ok(())
    }

    pub fn play_row(&self, index: usize) {
        let timeline = self.timeline();
        let timeline_cursor_pos = timeline.cursor_pos();
        let args = ColumnPlayRowArgs {
            slot_index: index,
            timeline,
            ref_pos: timeline_cursor_pos,
        };
        for handle in &self.column_handles {
            handle.command_sender.play_row(args.clone());
        }
    }

    fn timeline(&self) -> HybridTimeline {
        clip_timeline(self.project, false)
    }

    pub fn pause_slot(&self, coordinates: ClipSlotAddress) -> ClipEngineResult<()> {
        let handle = self.column_handle(coordinates.column())?;
        handle.command_sender.pause_slot(coordinates.row());
        Ok(())
    }

    pub fn column(&self, index: usize) -> ClipEngineResult<SharedColumn> {
        self.column_internal(index)
    }

    fn column_internal(&self, index: usize) -> ClipEngineResult<SharedColumn> {
        let handle = self.column_handle(index)?;
        handle
            .pointer
            .upgrade()
            .ok_or("column doesn't exist anymore")
    }

    fn columns_internal(&self) -> impl Iterator<Item = SharedColumn> + '_ {
        self.column_handles.iter().flat_map(|h| h.pointer.upgrade())
    }

    fn column_handle(&self, index: usize) -> ClipEngineResult<&ColumnHandle> {
        self.column_handles.get(index).ok_or("column doesn't exist")
    }
}

pub enum MatrixCommand {
    SetColumnHandles(Vec<ColumnHandle>),
}

pub trait RtMatrixCommandSender {
    fn set_column_handles(&self, handles: Vec<ColumnHandle>);
    fn send_command(&self, command: MatrixCommand);
}

impl RtMatrixCommandSender for Sender<MatrixCommand> {
    fn set_column_handles(&self, handles: Vec<ColumnHandle>) {
        self.send_command(MatrixCommand::SetColumnHandles(handles));
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
