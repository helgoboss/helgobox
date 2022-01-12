use crate::domain::{Timeline, TimelineMoment};
use enumflags2::BitFlags;
use reaper_high::{OwnedSource, Project, Reaper, Track};
use reaper_low::raw;
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    create_custom_owned_pcm_source, BorrowedPcmSource, Bpm, DurationInSeconds,
    FlexibleOwnedPcmSource, MeasureAlignment, MeasureMode, OwnedPreviewRegister, PlayState,
    PositionInBeats, PositionInSeconds, ReaperMutex, ReaperMutexGuard, ReaperVolumeValue,
};
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;

use helgoboss_learn::{UnitValue, BASE_EPSILON};

use crate::domain::clip::clip_source::{
    ClipPcmSource, ClipPcmSourceSkills, ClipPlayTime, ClipState, ClipStopTime, PlayArgs,
    PosWithinClipArgs, Repetition, SeekToArgs, SetRepeatedArgs, StopArgs, SuspensionReason,
};
use crate::domain::clip::{
    clip_timeline, clip_timeline_cursor_pos, Clip, ClipChangedEvent, ClipContent, ClipPlayState,
};

/// Represents an actually playable clip slot.
///
/// One clip slot corresponds to one REAPER preview register.
#[derive(Debug)]
pub struct ClipSlot {
    index: u32,
    clip: Clip,
    register: SharedRegister,
    state: State,
}

type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

/// Creates a REAPER preview register with its initial settings taken from the given descriptor.
fn create_shared_register(descriptor: &Clip) -> SharedRegister {
    let mut register = OwnedPreviewRegister::default();
    register.set_volume(descriptor.volume);
    register.set_out_chan(-1);
    Arc::new(ReaperMutex::new(register))
}

impl ClipSlot {
    pub fn new(index: u32) -> Self {
        let descriptor = Clip::default();
        let register = create_shared_register(&descriptor);
        Self {
            index,
            clip: descriptor,
            register,
            state: State::Empty,
        }
    }

    /// Returns the slot descriptor.
    pub fn descriptor(&self) -> &Clip {
        &self.clip
    }

    /// Empties the slot and resets all settings to the defaults (including volume, repeat etc.).
    ///
    /// Stops playback if necessary.
    pub fn reset(&mut self) -> Result<Vec<ClipChangedEvent>, &'static str> {
        self.load(Default::default(), None)
    }

    /// Loads all slot settings from the given descriptor (including the contained clip).
    ///
    /// Stops playback if necessary.
    pub fn load(
        &mut self,
        descriptor: Clip,
        project: Option<Project>,
    ) -> Result<Vec<ClipChangedEvent>, &'static str> {
        self.clear()?;
        // Using a completely new register saves us from cleaning up.
        self.register = create_shared_register(&descriptor);
        self.clip = descriptor;
        // If we can't load now, don't complain. Maybe media is missing just temporarily. Don't
        // mess up persistent data.
        let _ = self.load_content_from_descriptor(project);
        let events = vec![
            self.play_state_changed_event(),
            self.volume_changed_event(),
            self.repeat_changed_event(),
        ];
        Ok(events)
    }

    fn load_content_from_descriptor(
        &mut self,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        let source = if let Some(content) = self.clip.content.as_ref() {
            content.create_source(project)?
        } else {
            // Nothing to load
            return Ok(());
        };
        self.fill_with_source(source, project)?;
        Ok(())
    }

    /// Fills this slot with the given content, triggered by a user interaction.
    pub fn fill_by_user(
        &mut self,
        content: ClipContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        let source = content.create_source(project)?;
        self.fill_with_source(source, project)?;
        // Here it's important to not set the descriptor (change things) unless load was successful.
        self.clip.content = Some(content);
        Ok(())
    }

    fn fill_with_source(
        &mut self,
        source: OwnedSource,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .fill_with_source(source, &self.register, project);
        self.finish_transition(result)
    }

    /// Returns static information about the clip contained in this slot.
    pub fn clip_info(&self) -> Option<ClipInfo> {
        let guard = self.register.lock().ok()?;
        let source = guard.src()?;
        let source = source.as_ref();
        let info = ClipInfo {
            r#type: source.get_type(|t| t.to_string()),
            file_name: source.get_file_name(|p| Some(p?.to_owned())),
            length: {
                // TODO-low Doesn't need to be optional
                Some(source.native_clip_length())
            },
        };
        // TODO-medium This is probably necessary to make sure the mutex is not unlocked before the
        //  PCM source operations are done. How can we solve this in a better way API-wise? On the
        //  other hand, we are on our own anyway when it comes to PCM source thread safety ...
        std::mem::drop(guard);
        Some(info)
    }

    /// This method should be called regularly. It does the following:
    ///
    /// - Detects when the clip actually starts playing, has finished playing, and so on.
    /// - Changes the internal state accordingly.
    /// - Returns change events that inform about these state changes.
    /// - Returns a position change event if the state remained unchanged.
    ///
    /// No matter if the consumer needs the returned value or not, it *should* call this method
    /// regularly, because the clip start/stop mechanism relies on polling. This is not something to
    /// worry about because in practice, consumers always need to be informed about position changes.
    /// Performance-wise there's no need to implement change-event-based mechanism to detect
    /// clip start/stop (e.g. with channels). Because even if we would implement this, we would
    /// still want to keep getting informed about fine-grained position changes for UI/feedback
    /// updates. That means we would query slot information anyway every few milliseconds.
    /// Having the change-event-based mechanisms *in addition* to that would make performance even
    /// worse. The poll-based solution ensures that we can do all of the important
    /// stuff in one go and even summarize many changes easily into one batch before sending it
    /// to UI/controllers/clients.
    pub fn poll(
        &mut self,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> Option<ClipChangedEvent> {
        let (result, change_events) =
            self.start_transition()
                .poll(&self.register, timeline_cursor_pos, timeline_tempo);
        self.finish_transition(result).ok()?;
        change_events
    }

    /// Returns whether there's anything at all in this slot.
    pub fn is_filled(&self) -> bool {
        self.clip.is_filled()
    }

    /// A slot can be filled but the source might not be loaded.
    pub fn source_is_loaded(&self) -> bool {
        !matches!(self.state, State::Empty)
    }

    /// Returns the play state of this slot, derived from the slot state.  
    pub fn play_state(&self) -> ClipPlayState {
        self.state
            .play_state(&self.register, clip_timeline_cursor_pos(None))
    }

    /// Generates a change event from the current play state of this slot.
    fn play_state_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::PlayState(self.play_state())
    }

    /// Instructs this slot to play the contained clip.
    ///
    /// The clip might start immediately or on the next bar, depending on the play options.
    ///
    /// It only returns a change event if the previous state was stopped or paused (because in these
    /// states we don't poll).  
    pub fn play(
        &mut self,
        project: Project,
        track: Option<Track>,
        options: SlotPlayOptions,
        moment: TimelineMoment,
    ) -> Result<(), &'static str> {
        let result = self.start_transition().play(
            &self.register,
            ClipPlayArgs {
                project,
                options,
                track,
                repeat: self.clip.repeat,
            },
            moment,
        );
        self.finish_transition(result)
    }

    /// Stops playback if necessary, destroys the contained source and resets the playback position
    /// to zero.
    pub fn clear(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().clear(&self.register);
        self.finish_transition(result)
    }

    /// This method should be called whenever REAPER's play state changes. It will make the clip
    /// start/stop synchronized with REAPER's transport.
    pub fn process_transport_change(
        &mut self,
        new_play_state: PlayState,
        moment: TimelineMoment,
        timeline: &impl Timeline,
    ) -> Result<Option<ClipChangedEvent>, &'static str> {
        let result = self.start_transition().process_transport_change(
            &self.register,
            new_play_state,
            moment,
            timeline,
        );
        self.finish_transition(result)?;
        Ok(Some(self.play_state_changed_event()))
    }

    /// Instructs this slot to stop the contained clip.
    ///
    /// Either immediately or when it has finished playing.
    pub fn stop(
        &mut self,
        stop_behavior: SlotStopBehavior,
        moment: TimelineMoment,
    ) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .stop(&self.register, stop_behavior, moment);
        self.finish_transition(result)
    }

    /// Pauses clip playing.
    pub fn pause(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().pause(&self.register);
        self.finish_transition(result)
    }

    /// Returns whether repeat is enabled for this clip.
    pub fn repeat_is_enabled(&self) -> bool {
        self.clip.repeat
    }

    fn repeat_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::ClipRepeat(self.clip.repeat)
    }

    /// Toggles repeat for the slot clip.
    pub fn toggle_repeat(&mut self) -> ClipChangedEvent {
        let new_value = !self.clip.repeat;
        self.clip.repeat = new_value;
        let timeline = clip_timeline(None);
        let args = SetRepeatedArgs {
            timeline_cursor_pos: timeline.cursor_pos(),
            timeline_tempo: timeline.tempo(),
            repeated: new_value,
        };
        let mut guard = lock(&self.register);
        if let Some(src) = guard.src_mut() {
            src.as_mut().set_repeated(args);
        }
        self.repeat_changed_event()
    }

    /// Returns the volume of the slot clip.
    pub fn volume(&self) -> ReaperVolumeValue {
        self.clip.volume
    }

    fn volume_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::ClipVolume(self.clip.volume)
    }

    /// Sets volume of the slot clip.
    pub fn set_volume(&mut self, volume: ReaperVolumeValue) -> ClipChangedEvent {
        self.clip.volume = volume;
        lock(&self.register).set_volume(volume);
        self.volume_changed_event()
    }

    /// Returns the tempo factor of the slot clip.
    pub fn tempo_factor(&self) -> f64 {
        let guard = lock(&self.register);
        if let Some(src) = guard.src() {
            src.as_ref().get_tempo_factor()
        } else {
            1.0
        }
    }

    /// Sets tempo factor of the slot clip.
    pub fn set_tempo_factor(&mut self, tempo_factor: f64) -> Result<(), &'static str> {
        let mut guard = lock(&self.register);
        let src = guard.src_mut().ok_or(NO_SOURCE_LOADED)?;
        src.as_mut().set_tempo_factor(tempo_factor);
        Ok(())
    }

    /// Returns the current position within the slot clip on a percentage basis.
    pub fn proportional_position(&self) -> Result<UnitValue, &'static str> {
        let timeline = clip_timeline(None);
        let args = PosWithinClipArgs {
            timeline_cursor_pos: timeline.cursor_pos(),
            timeline_tempo: timeline.tempo(),
        };
        let guard = lock(&self.register);
        let src = guard.src().ok_or(NO_SOURCE_LOADED)?;
        if matches!(self.state, State::Empty) {
            return Ok(UnitValue::MIN);
        }
        let src = src.as_ref();
        let percentage_pos = src.proportional_pos_within_clip(args).unwrap_or_default();
        Ok(percentage_pos)
    }

    /// Returns the current clip position in seconds.
    pub fn position_in_seconds(&self) -> Result<PositionInSeconds, &'static str> {
        let timeline = clip_timeline(None);
        let args = PosWithinClipArgs {
            timeline_cursor_pos: timeline.cursor_pos(),
            timeline_tempo: timeline.tempo(),
        };
        let guard = lock(&self.register);
        let src = guard.src().ok_or(NO_SOURCE_LOADED)?;
        if matches!(self.state, State::Empty) {
            return Ok(PositionInSeconds::ZERO);
        }
        let pos = src.as_ref().pos_within_clip(args).unwrap_or_default();
        Ok(pos)
    }

    /// Changes the clip position on a percentage basis.
    ///
    /// Only fires a change event if this clip is paused (because then position changes can happen
    /// only by using this method so we don't need and don't use polling to detect them).
    pub fn set_proportional_position(
        &mut self,
        desired_proportional_pos: UnitValue,
    ) -> Result<Option<ClipChangedEvent>, &'static str> {
        let timeline = clip_timeline(None);
        let args = SeekToArgs {
            timeline_cursor_pos: timeline.cursor_pos(),
            timeline_tempo: timeline.tempo(),
            desired_pos: desired_proportional_pos,
        };
        let mut guard = lock(&self.register);
        let src = guard.src_mut().ok_or(NO_SOURCE_LOADED)?;
        let src = src.as_mut();
        let clip_state = src.clip_state();
        src.seek_to(args);
        let event = if matches!(clip_state, ClipState::Paused { .. }) {
            Some(ClipChangedEvent::ClipPosition(desired_proportional_pos))
        } else {
            // Either stopped (then we can't seek) or playing (then position changes are
            // detected by polling).
            None
        };
        Ok(event)
    }

    fn start_transition(&mut self) -> State {
        std::mem::replace(&mut self.state, State::Transitioning)
    }

    fn finish_transition(&mut self, result: TransitionResult) -> Result<(), &'static str> {
        let (next_state, result) = match result {
            Ok(s) => (s, Ok(())),
            Err((s, msg)) => (s, Err(msg)),
        };
        self.state = next_state;
        result
    }
}

type TransitionResult = Result<State, (State, &'static str)>;

/// The internal state of a slot.
#[derive(Debug)]
enum State {
    Empty,
    Filled(FilledState),
    Transitioning,
}

impl State {
    /// Derives the corresponding clip play state.
    pub fn play_state(
        &self,
        reg: &SharedRegister,
        timeline_cursor_pos: PositionInSeconds,
    ) -> ClipPlayState {
        use State::*;
        match self {
            Empty => ClipPlayState::Stopped,
            Filled(s) => s.play_state(reg, timeline_cursor_pos),
            Transitioning => unreachable!(),
        }
    }

    pub fn process_transport_change(
        self,
        reg: &SharedRegister,
        new_play_state: PlayState,
        moment: TimelineMoment,
        timeline: &impl Timeline,
    ) -> TransitionResult {
        if !timeline.follows_reaper_transport() {
            return Ok(self);
        }
        match self {
            State::Empty => Ok(State::Empty),
            State::Filled(mut s) => {
                let change = RelevantTransportChange::from_play_state_change(
                    s.last_project_play_state,
                    new_play_state,
                );
                s.last_project_play_state = new_play_state;
                let change = match change {
                    None => return Ok(State::Filled(s)),
                    Some(c) => c,
                };
                // We have a relevant transport change.
                let play_args = match s.last_play_args.clone() {
                    None => return Ok(State::Filled(s)),
                    Some(a) => a,
                };
                // Clip was started once already.
                let synced = play_args.options.next_bar;
                // Clip was started in sync with project.
                let state = s.play_state(reg, moment.cursor_pos());
                use ClipPlayState::*;
                // Pausing the transport makes the complete timeline pause, so we don't need to
                // do anything here.
                match change {
                    RelevantTransportChange::PlayAfterStop => {
                        match state {
                            Stopped | Paused if s.was_caused_by_transport_change => {
                                // REAPER transport was started, either from stopped or paused
                                // state. Clip is either stopped or paused as well and was put in
                                // that state due to a previous transport stop or pause.
                                // Play the clip! It should automatically do the right thing, that
                                // is schedule or resume, depending in which state it was.
                                s.play(reg, play_args, moment)
                            }
                            _ => {
                                // Stop and forget (because we have a timeline switch).
                                s.stop(reg, SlotStopBehavior::Immediately, false, moment)
                            }
                        }
                    }
                    RelevantTransportChange::StopAfterPlay => match state {
                        ScheduledForPlay | Playing | ScheduledForStop if synced => {
                            // Stop and memorize
                            s.stop(reg, SlotStopBehavior::Immediately, true, moment)
                        }
                        _ => {
                            // Stop and forget
                            s.stop(reg, SlotStopBehavior::Immediately, false, moment)
                        }
                    },
                    RelevantTransportChange::StopAfterPause => {
                        s.stop(reg, SlotStopBehavior::Immediately, false, moment)
                    }
                }
            }
            State::Transitioning => unreachable!(),
        }
    }

    pub fn play(
        self,
        reg: &SharedRegister,
        args: ClipPlayArgs,
        moment: TimelineMoment,
    ) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((Empty, "slot is empty")),
            Filled(s) => s.play(reg, args, moment),
            Transitioning => unreachable!(),
        }
    }

    pub fn stop(
        self,
        reg: &SharedRegister,
        stop_behavior: SlotStopBehavior,
        moment: TimelineMoment,
    ) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Filled(s) => s.stop(reg, stop_behavior, false, moment),
            Transitioning => unreachable!(),
        }
    }

    pub fn pause(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Filled(s) => s.pause(reg, false, clip_timeline_cursor_pos(None)),
            Transitioning => unreachable!(),
        }
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Filled(s) => s.clear(reg),
            Transitioning => unreachable!(),
        }
    }

    pub fn poll(
        self,
        reg: &SharedRegister,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> (TransitionResult, Option<ClipChangedEvent>) {
        use State::*;
        match self {
            Filled(s) => s.poll(reg, timeline_cursor_pos, timeline_tempo),
            // When empty, change nothing and emit no change events.
            Empty => (Ok(self), None),
            Transitioning => unreachable!(),
        }
    }

    pub fn fill_with_source(
        self,
        source: OwnedSource,
        reg: &SharedRegister,
        project: Option<Project>,
    ) -> TransitionResult {
        let source = ClipPcmSource::new(source.into_raw(), project);
        let source = create_custom_owned_pcm_source(source);
        let source = FlexibleOwnedPcmSource::Custom(source);
        use State::*;
        match self {
            Empty => {
                let mut g = lock(reg);
                g.set_src(Some(source));
                let new_state = FilledState {
                    last_project_play_state: {
                        project
                            .unwrap_or_else(|| Reaper::get().current_project())
                            .play_state()
                    },
                    last_clip_play_state: ClipPlayState::Stopped,
                    handle: None,
                    last_play_args: None,
                    was_caused_by_transport_change: false,
                };
                Ok(State::Filled(new_state))
            }
            Filled(s) => s.fill_with_source(source, reg),
            Transitioning => unreachable!(),
        }
    }
}

fn start_playing_preview(
    track: Option<&Track>,
    reg: &SharedRegister,
) -> Result<NonNull<raw::preview_register_t>, &'static str> {
    // TODO-high We might want to buffer.
    let buffering_behavior = BitFlags::empty();
    let measure_alignment = MeasureAlignment::PlayImmediately;
    let result = if let Some(track) = track {
        Reaper::get().medium_session().play_track_preview_2_ex(
            track.project().context(),
            reg.clone(),
            buffering_behavior,
            measure_alignment,
        )
    } else {
        Reaper::get().medium_session().play_preview_ex(
            reg.clone(),
            buffering_behavior,
            measure_alignment,
        )
    };
    result.map_err(|_| "couldn't play preview")
}

fn stop_playing_preview(track: Option<&Track>, handle: NonNull<raw::preview_register_t>) {
    if let Some(track) = track.as_ref() {
        // Check prevents error message on project close.
        let project = track.project();
        if project.is_available() {
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get()
                .medium_session()
                .stop_track_preview_2(project.context(), handle);
        }
    } else {
        // If not successful this probably means it was stopped already, so okay.
        let _ = Reaper::get().medium_session().stop_preview(handle);
    };
}

#[derive(Debug)]
struct FilledState {
    // TODO-high This could be saved elsewhere, just once, e.g. in InstanceState
    last_project_play_state: PlayState,
    last_clip_play_state: ClipPlayState,
    handle: Option<NonNull<raw::preview_register_t>>,
    last_play_args: Option<ClipPlayArgs>,
    was_caused_by_transport_change: bool,
}

#[derive(Clone, Debug)]
struct ClipPlayArgs {
    project: Project,
    options: SlotPlayOptions,
    track: Option<Track>,
    repeat: bool,
}

impl FilledState {
    pub fn fill_with_source(
        self,
        source: FlexibleOwnedPcmSource,
        reg: &SharedRegister,
    ) -> TransitionResult {
        let mut g = lock(reg);
        g.set_src(Some(source));
        Ok(State::Filled(self))
    }

    pub fn play(
        self,
        reg: &SharedRegister,
        args: ClipPlayArgs,
        moment: TimelineMoment,
    ) -> TransitionResult {
        {
            let mut guard = lock(reg);
            // Handle preview track.
            if let Some(last_play_args) = &self.last_play_args {
                // We played this clip before.
                if last_play_args.track.as_ref() != args.track.as_ref() {
                    // The of this play is different from the track of the last play.
                    // TODO-high Handle track change decently. If we are currently playing, we can't
                    //  just change the preview register track. We need to send all notes off before
                    //  (TransitioningToTrackChange) and put the source into a ReadyForTrackChange
                    //  state. We detect that by slot polling and can then change the preview register
                    //  track and call the source with ContinueAfterTrackChange.
                }
            } else {
                // We haven't played this clip already. Set preview track!
                guard.set_preview_track(args.track.as_ref().map(|t| t.raw()));
            }
            // Start clip.
            let timeline_cursor_pos = clip_timeline_cursor_pos(Some(args.project));
            let src = guard.src_mut().expect(NO_SOURCE_LOADED);
            let play_args = PlayArgs {
                timeline_cursor_pos,
                play_time: if args.options.next_bar {
                    ClipPlayTime::NextBar
                } else {
                    ClipPlayTime::Immediately
                },
                repetition: Repetition::from_bool(args.repeat),
            };
            src.as_mut().play(play_args);
        }
        let was_caused_by_transport_change = self.was_caused_by_transport_change;
        let last_play_state = self.last_clip_play_state;
        let last_project_play_state = self.last_project_play_state;
        let handle = if let Some(handle) = self.handle {
            // Preview register playing already.
            handle
        } else {
            // Preview register not playing yet. Start playing!
            start_playing_preview(args.track.as_ref(), reg)
                .map_err(|text| (State::Filled(self), text))?
        };
        let next_state = FilledState {
            last_project_play_state,
            handle: Some(handle),
            last_play_args: Some(args),
            was_caused_by_transport_change,
            last_clip_play_state: last_play_state,
        };
        Ok(State::Filled(next_state))
    }

    pub fn pause(
        self,
        reg: &SharedRegister,
        was_caused_by_transport_change: bool,
        timeline_cursor_pos: PositionInSeconds,
    ) -> TransitionResult {
        let mut guard = lock(reg);
        if let Some(src) = guard.src_mut() {
            src.as_mut().pause(timeline_cursor_pos);
        }
        let next_state = FilledState {
            was_caused_by_transport_change,
            ..self
        };
        Ok(State::Filled(next_state))
    }

    pub fn stop(
        self,
        reg: &SharedRegister,
        stop_behavior: SlotStopBehavior,
        was_caused_by_transport_change: bool,
        moment: TimelineMoment,
    ) -> TransitionResult {
        let mut guard = lock(reg);
        if let Some(src) = guard.src_mut() {
            use SlotStopBehavior::*;
            let stop_args = StopArgs {
                timeline_cursor_pos: moment.cursor_pos(),
                timeline_tempo: moment.tempo(),
                pos: {
                    match stop_behavior {
                        Immediately => ClipStopTime::Immediately,
                        EndOfBar | EndOfClip => stop_behavior.get_clip_stop_position(moment),
                    }
                },
            };
            src.as_mut().stop(stop_args);
        }
        let next_state = FilledState {
            was_caused_by_transport_change,
            ..self
        };
        Ok(State::Filled(next_state))
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        // TODO-high Handle this decently. If we are currently playing, we can't
        //  just clear the source. We need to send all notes off before
        //  (TransitioningToSourceChange) and put the source into a ReadyForSourceChange
        //  state. We detect that by slot polling and can then clear the source.
        let mut g = lock(reg);
        g.set_src(None);
        Ok(State::Empty)
    }

    pub fn play_state(
        &self,
        reg: &SharedRegister,
        timeline_cursor_pos: PositionInSeconds,
    ) -> ClipPlayState {
        let guard = lock(reg);
        let src = guard.src().expect(NO_SOURCE_LOADED).as_ref();
        let clip_state = src.clip_state();
        get_play_state(src, timeline_cursor_pos, clip_state)
    }

    pub fn poll(
        self,
        reg: &SharedRegister,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> (TransitionResult, Option<ClipChangedEvent>) {
        let handle = match self.handle {
            None => {
                // If we don't have a preview register handle, the clip is stopped or paused and we can
                // return early. No polling necessary.
                return (Ok(State::Filled(self)), None);
            }
            Some(h) => h,
        };
        // At first collect some up-to-date clip source state.
        // TODO-high We should optimize this by getting everything at once.
        let (clip_state, play_state, pos_within_clip) = {
            let args = PosWithinClipArgs {
                timeline_cursor_pos,
                timeline_tempo,
            };
            // React gracefully even in weird situations (because we are in poll).
            let guard = match reg.lock() {
                Ok(g) => g,
                Err(_) => return (Ok(State::Filled(self)), None),
            };
            let src = match guard.src() {
                Some(s) => s,
                None => return (Ok(State::Filled(self)), None),
            };
            let src = src.as_ref();
            let pos_within_clip = src.proportional_pos_within_clip(args);
            let clip_state = src.clip_state();
            let play_state = get_play_state(src, timeline_cursor_pos, clip_state);
            (clip_state, play_state, pos_within_clip)
        };
        // At first determine event to be emitted.
        let next_event = if play_state == self.last_clip_play_state {
            // Play state has not changed. If we are polling, it means the position is changing
            // constantly. Emit it!
            Some(ClipChangedEvent::ClipPosition(
                pos_within_clip.unwrap_or_default(),
            ))
        } else {
            // Play state has changed. Emit this instead of a position change.
            Some(ClipChangedEvent::PlayState(play_state))
        };
        // Then determine next state.
        let next_state = {
            // At first check if it's time to stop the preview register (to save resources).
            use ClipState::*;
            let next_handle = match clip_state {
                // We still have a playing preview register but the clip got stopped or paused.
                // Stop it!
                Stopped | Paused { .. } => {
                    stop_playing_preview(
                        self.last_play_args.as_ref().and_then(|a| a.track.as_ref()),
                        handle,
                    );
                    None
                }
                // We are still playing and need the preview register to keep playing.
                ScheduledOrPlaying { .. } | Suspending { .. } => Some(handle),
            };
            // Then build new state.
            let filled_state = FilledState {
                last_clip_play_state: play_state,
                handle: next_handle,
                ..self
            };
            State::Filled(filled_state)
        };
        (Ok(next_state), next_event)
    }
}

/// Contains static information about a clip.
pub struct ClipInfo {
    pub r#type: String,
    pub file_name: Option<PathBuf>,
    pub length: Option<DurationInSeconds>,
}

/// Defines how to stop the clip.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SlotStopBehavior {
    Immediately,
    EndOfBar,
    EndOfClip,
}

impl SlotStopBehavior {
    fn get_clip_stop_position(&self, moment: TimelineMoment) -> ClipStopTime {
        use SlotStopBehavior::*;
        match self {
            EndOfBar => ClipStopTime::At(moment.next_bar_pos()),
            EndOfClip => ClipStopTime::AtEndOfClip,
            Immediately => unimplemented!("not used"),
        }
    }
}

/// Contains instructions how to play a clip.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {
    /// Syncs with timeline.
    pub next_bar: bool,
    pub buffered: bool,
}

impl SlotPlayOptions {
    pub fn is_effectively_buffered(&self) -> bool {
        // Observation: buffered must be on if next bar is enabled.
        self.buffered || self.next_bar
    }
}

fn lock(reg: &SharedRegister) -> ReaperMutexGuard<OwnedPreviewRegister> {
    reg.lock().expect("couldn't acquire lock")
}

fn get_play_state(
    src: &BorrowedPcmSource,
    timeline_cursor_pos: PositionInSeconds,
    clip_state: ClipState,
) -> ClipPlayState {
    use ClipState::*;
    match clip_state {
        Stopped => ClipPlayState::Stopped,
        ScheduledOrPlaying {
            play_info,
            stop_instruction,
            ..
        } => {
            if stop_instruction.is_some() {
                ClipPlayState::ScheduledForStop
            } else if let Some(play_info) = play_info {
                if play_info.next_block_pos < PositionInSeconds::ZERO {
                    ClipPlayState::ScheduledForPlay
                } else {
                    ClipPlayState::Playing
                }
            } else {
                ClipPlayState::ScheduledForPlay
            }
        }
        Suspending { reason, .. } => match reason {
            SuspensionReason::Retrigger | SuspensionReason::PlayWhileSuspending { .. } => {
                ClipPlayState::Playing
            }
            SuspensionReason::Pause => ClipPlayState::Paused,
            SuspensionReason::Stop => ClipPlayState::Stopped,
        },
        Paused { .. } => ClipPlayState::Paused,
    }
}

const NO_SOURCE_LOADED: &str = "no source loaded";

#[derive(Debug)]
enum RelevantTransportChange {
    PlayAfterStop,
    StopAfterPlay,
    StopAfterPause,
}

impl RelevantTransportChange {
    fn from_play_state_change(old: PlayState, new: PlayState) -> Option<Self> {
        use RelevantTransportChange::*;
        let change = if !old.is_paused && !old.is_playing && new.is_playing {
            PlayAfterStop
        } else if old.is_playing && !new.is_playing && !new.is_paused {
            StopAfterPlay
        } else if old.is_paused && !new.is_playing && !new.is_paused {
            StopAfterPause
        } else {
            return None;
        };
        Some(change)
    }
}
