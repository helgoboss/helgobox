use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;

use enumflags2::BitFlags;
use reaper_high::{OwnedSource, Project, Reaper, Track};
use reaper_low::raw;
use reaper_medium::{
    create_custom_owned_pcm_source, BorrowedPcmSource, DurationInSeconds, FlexibleOwnedPcmSource,
    MeasureAlignment, MeasureMode, OwnedPreviewRegister, PlayState, PositionInBeats,
    PositionInSeconds, ReaperMutex, ReaperMutexGuard, ReaperVolumeValue,
};

use helgoboss_learn::{UnitValue, BASE_EPSILON};

use crate::domain::clip::clip_source::{
    ClipPcmSource, ClipPcmSourceSkills, ClipState, ClipStopPosition, RunPhase,
};
use crate::domain::clip::{Clip, ClipChangedEvent, ClipContent, ClipPlayState};

/// Represents an actually playable clip slot.
///
/// One clip slot corresponds to one REAPER preview register.
#[derive(Debug)]
pub struct ClipSlot {
    clip: Clip,
    register: SharedRegister,
    state: State,
}

type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

impl Default for ClipSlot {
    fn default() -> Self {
        let descriptor = Clip::default();
        let register = create_shared_register(&descriptor);
        Self {
            clip: descriptor,
            register,
            state: State::Empty,
        }
    }
}

/// Creates a REAPER preview register with its initial settings taken from the given descriptor.
fn create_shared_register(descriptor: &Clip) -> SharedRegister {
    let mut register = OwnedPreviewRegister::default();
    register.set_volume(descriptor.volume);
    register.set_out_chan(-1);
    Arc::new(ReaperMutex::new(register))
}

impl ClipSlot {
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
        self.fill_with_source(source)?;
        Ok(())
    }

    /// Fills this slot with the given content, triggered by a user interaction.
    pub fn fill_by_user(
        &mut self,
        content: ClipContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        let source = content.create_source(project)?;
        self.fill_with_source(source)?;
        // Here it's important to not set the descriptor (change things) unless load was successful.
        self.clip.content = Some(content);
        Ok(())
    }

    fn fill_with_source(&mut self, source: OwnedSource) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .fill_with_source(source, &self.register);
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
                Some(source.inner_length())
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
    pub fn poll(&mut self) -> Option<ClipChangedEvent> {
        let (result, change_events) = self.start_transition().poll(&self.register);
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
        self.state.play_state(&self.register)
    }

    /// Generates a change event from the current play state of this slot.
    fn play_state_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::PlayState(self.play_state())
    }

    /// Instructs this slot to play the contained clip.
    ///
    /// The clip might start immediately or on the next bar, depending on the play options.  
    pub fn play(
        &mut self,
        project: Project,
        track: Option<Track>,
        options: SlotPlayOptions,
    ) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().play(
            &self.register,
            ClipPlayArgs {
                project,
                options,
                track,
                repeat: self.clip.repeat,
            },
        );
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
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
        project: Project,
        new_play_state: PlayState,
    ) -> Result<Option<ClipChangedEvent>, &'static str> {
        if !self.clip.repeat {
            // One-shots should not be synchronized with main timeline.
            return Ok(None);
        }
        let result = self.start_transition().process_transport_change(
            &self.register,
            new_play_state,
            project,
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
        project: Project,
    ) -> Result<ClipChangedEvent, &'static str> {
        let result = self
            .start_transition()
            .stop(&self.register, stop_behavior, project);
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
    }

    /// Pauses clip playing.
    pub fn pause(&mut self) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().pause(&self.register);
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
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
        let mut guard = lock(&self.register);
        if let Some(src) = guard.src_mut() {
            src.as_mut().set_repeated(new_value);
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

    /// Returns the current position within the slot clip on a percentage basis.
    pub fn proportional_position(&self) -> Result<UnitValue, &'static str> {
        let guard = lock(&self.register);
        let src = guard.src().ok_or(NO_SOURCE_LOADED)?;
        if matches!(self.state, State::Empty) {
            return Ok(UnitValue::MIN);
        }
        let src = src.as_ref();
        let pos_within_clip = src.pos_within_clip();
        let length = src.inner_length();
        let percentage_pos = calculate_proportional_position(pos_within_clip, length);
        Ok(percentage_pos)
    }

    /// Returns the current clip position in seconds.
    pub fn position_in_seconds(&self) -> Result<PositionInSeconds, &'static str> {
        let guard = lock(&self.register);
        let src = guard.src().ok_or(NO_SOURCE_LOADED)?;
        if matches!(self.state, State::Empty) {
            return Ok(PositionInSeconds::ZERO);
        }
        Ok(src.as_ref().pos_within_clip().unwrap_or_default())
    }

    /// Changes the clip position on a percentage basis.
    pub fn set_proportional_position(
        &mut self,
        desired_proportional_pos: UnitValue,
    ) -> Result<ClipChangedEvent, &'static str> {
        let mut guard = lock(&self.register);
        let source = guard.src_mut().ok_or(NO_SOURCE_LOADED)?;
        let source = source.as_mut();
        let length = source.inner_length();
        let desired_pos_in_secs =
            PositionInSeconds::new(desired_proportional_pos.get() * length.get());
        source.seek_to(desired_pos_in_secs);
        Ok(ClipChangedEvent::ClipPosition(desired_proportional_pos))
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
    pub fn play_state(&self, reg: &SharedRegister) -> ClipPlayState {
        use State::*;
        match self {
            Empty => ClipPlayState::Stopped,
            Filled(s) => s.play_state(reg),
            Transitioning => unreachable!(),
        }
    }

    pub fn process_transport_change(
        self,
        reg: &SharedRegister,
        new_play_state: PlayState,
        project: Project,
    ) -> TransitionResult {
        match self {
            State::Empty => Ok(State::Empty),
            State::Filled(s) => {
                if let Some(play_args) = s.last_play_args.clone() {
                    // Clip was started once already.
                    if play_args.options.next_bar {
                        // Clip was started in sync with project.
                        // TODO-high The reactions here are sometimes not what we want.
                        //  Define for each initial clip state what we would expect first!
                        use ClipPlayState::*;
                        match s.play_state(reg) {
                            Stopped | Paused
                                if s.was_caused_by_transport_change
                                    && new_play_state.is_playing
                                    && !new_play_state.is_paused =>
                            {
                                // Clip is stopped/paused due to previous transport change.
                                // REAPER transport was started from stop or paused state.
                                // Play the clip (synchronized to the next bar).
                                s.play(reg, play_args)
                            }
                            ScheduledForPlay | Playing | ScheduledForStop => {
                                // Clip is playing
                                if new_play_state.is_playing && !new_play_state.is_paused {
                                    // REAPER transport was started from stop or paused state.
                                    Ok(State::Filled(s))
                                } else if new_play_state.is_paused {
                                    s.pause(reg, true)
                                } else {
                                    s.stop(reg, SlotStopBehavior::Immediately, true, project)
                                }
                            }
                            _ => Ok(State::Filled(s)),
                        }
                    } else {
                        Ok(State::Filled(s))
                    }
                } else {
                    Ok(State::Filled(s))
                }
            }
            State::Transitioning => unreachable!(),
        }
    }

    pub fn play(self, reg: &SharedRegister, args: ClipPlayArgs) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((Empty, "slot is empty")),
            Filled(s) => s.play(reg, args),
            Transitioning => unreachable!(),
        }
    }

    pub fn stop(
        self,
        reg: &SharedRegister,
        stop_behavior: SlotStopBehavior,
        project: Project,
    ) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Filled(s) => s.stop(reg, stop_behavior, false, project),
            Transitioning => unreachable!(),
        }
    }

    pub fn pause(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Filled(s) => s.pause(reg, false),
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

    pub fn poll(self, reg: &SharedRegister) -> (TransitionResult, Option<ClipChangedEvent>) {
        use State::*;
        match self {
            Filled(s) => s.poll(reg),
            // When empty, change nothing and emit no change events.
            Empty => (Ok(self), None),
            Transitioning => unreachable!(),
        }
    }

    pub fn fill_with_source(self, source: OwnedSource, reg: &SharedRegister) -> TransitionResult {
        let source = ClipPcmSource::new(source.into_raw());
        let source = create_custom_owned_pcm_source(source);
        let source = FlexibleOwnedPcmSource::Custom(source);
        use State::*;
        match self {
            Empty => {
                let mut g = lock(reg);
                g.set_src(Some(source));
                let new_state = FilledState {
                    last_play_state: ClipPlayState::Stopped,
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
    last_play_state: ClipPlayState,
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

    pub fn play(self, reg: &SharedRegister, args: ClipPlayArgs) -> TransitionResult {
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
            if let Some(src) = guard.src_mut() {
                if args.options.next_bar {
                    let scheduled_pos = get_next_bar_pos(args.project);
                    src.as_mut().schedule_start(scheduled_pos, args.repeat);
                } else {
                    src.as_mut().start_immediately(args.repeat);
                };
            }
        }
        let was_caused_by_transport_change = self.was_caused_by_transport_change;
        let last_play_state = self.last_play_state;
        let handle = if let Some(handle) = self.handle {
            // Preview register playing already.
            handle
        } else {
            // Preview register not playing yet. Start playing!
            start_playing_preview(args.track.as_ref(), reg)
                .map_err(|text| (State::Filled(self), text))?
        };
        let next_state = FilledState {
            handle: Some(handle),
            last_play_args: Some(args),
            was_caused_by_transport_change,
            last_play_state,
        };
        Ok(State::Filled(next_state))
    }

    pub fn pause(self, reg: &SharedRegister, caused_by_transport_change: bool) -> TransitionResult {
        let mut guard = lock(reg);
        if let Some(src) = guard.src_mut() {
            src.as_mut().pause();
        }
        let next_state = FilledState {
            was_caused_by_transport_change: caused_by_transport_change,
            ..self
        };
        Ok(State::Filled(next_state))
    }

    pub fn stop(
        self,
        reg: &SharedRegister,
        stop_behavior: SlotStopBehavior,
        caused_by_transport_change: bool,
        project: Project,
    ) -> TransitionResult {
        let mut guard = lock(reg);
        if let Some(src) = guard.src_mut() {
            use SlotStopBehavior::*;
            match stop_behavior {
                Immediately => {
                    src.as_mut().stop_immediately();
                }
                EndOfBar | EndOfClip => {
                    src.as_mut()
                        .schedule_stop(stop_behavior.get_clip_stop_position(project));
                }
            }
        }
        let next_state = FilledState {
            was_caused_by_transport_change: caused_by_transport_change,
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

    pub fn play_state(&self, reg: &SharedRegister) -> ClipPlayState {
        let guard = lock(reg);
        let src = guard.src().expect(NO_SOURCE_LOADED).as_ref();
        get_play_state(src)
    }

    pub fn poll(self, reg: &SharedRegister) -> (TransitionResult, Option<ClipChangedEvent>) {
        // TODO-medium We can optimize this by getting everything at once.
        let (play_state, pos_within_clip, length) = {
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
            let pos_within_clip = src.pos_within_clip();
            let length = src.inner_length();
            let play_state = get_play_state(src);
            (play_state, pos_within_clip, length)
        };
        let (next_state, event) = if play_state == self.last_play_state {
            (Ok(State::Filled(self)), None)
        } else {
            use ClipPlayState::*;
            let remove_handle = match play_state {
                // TODO-high Problem: We will probably emit obsolete pos change events if stopped and
                //  paused.
                Stopped | Paused => {
                    if let Some(handle) = self.handle {
                        stop_playing_preview(
                            self.last_play_args.as_ref().and_then(|a| a.track.as_ref()),
                            handle,
                        );
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            let new_state = if remove_handle {
                FilledState {
                    last_play_state: play_state,
                    handle: None,
                    ..self
                }
            } else {
                FilledState {
                    last_play_state: play_state,
                    ..self
                }
            };
            (
                Ok(State::Filled(new_state)),
                Some(ClipChangedEvent::PlayState(play_state)),
            )
        };
        // If no other change event is detected, we emit the position.
        let final_event = event.unwrap_or_else(|| {
            let position = calculate_proportional_position(pos_within_clip, length);
            ClipChangedEvent::ClipPosition(position)
        });
        (next_state, Some(final_event))
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
    fn get_clip_stop_position(&self, project: Project) -> ClipStopPosition {
        use SlotStopBehavior::*;
        match self {
            EndOfBar => ClipStopPosition::At(get_next_bar_pos(project)),
            EndOfClip => ClipStopPosition::AtEndOfClip,
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

fn calculate_proportional_position(
    position: Option<PositionInSeconds>,
    length: DurationInSeconds,
) -> UnitValue {
    if length.get() == 0.0 {
        return UnitValue::MIN;
    }
    position
        .map(|p| UnitValue::new_clamped(p.get() / length.get()))
        .unwrap_or_default()
}

fn get_next_bar_pos(project: Project) -> PositionInSeconds {
    let reaper = Reaper::get().medium_reaper();
    let timeline_pos = project.play_position_next_audio_block();
    let proj_context = project.context();
    let res = reaper.time_map_2_time_to_beats(proj_context, timeline_pos);
    let next_measure_index = if res.beats_since_measure.get() <= BASE_EPSILON {
        res.measure_index
    } else {
        res.measure_index + 1
    };
    reaper.time_map_2_beats_to_time(
        proj_context,
        MeasureMode::FromMeasureAtIndex(next_measure_index),
        PositionInBeats::ZERO,
    )
}

fn get_play_state(src: &BorrowedPcmSource) -> ClipPlayState {
    match src.query_state() {
        ClipState::Stopped => ClipPlayState::Stopped,
        ClipState::Running(s) => {
            use RunPhase::*;
            match s.phase {
                ScheduledOrPlaying => {
                    if let Some(pos_from_start) = src.pos_from_start() {
                        if pos_from_start < PositionInSeconds::ZERO {
                            ClipPlayState::ScheduledForPlay
                        } else {
                            ClipPlayState::Playing
                        }
                    } else {
                        // TODO-high Improve
                        // Not running after all
                        ClipPlayState::Stopped
                    }
                }
                Retriggering => ClipPlayState::Playing,
                TransitioningToPause | Paused => ClipPlayState::Paused,
                ScheduledForStop(_) => ClipPlayState::ScheduledForStop,
                TransitioningToStop => ClipPlayState::Stopped,
            }
        }
    }
}

const NO_SOURCE_LOADED: &str = "no source loaded";
