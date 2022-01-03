use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;

use enumflags2::BitFlags;
use reaper_high::{OwnedSource, Project, Reaper, Track};
use reaper_low::raw;
use reaper_medium::{
    create_custom_owned_pcm_source, DurationInSeconds, FlexibleOwnedPcmSource, MeasureAlignment,
    MeasureMode, OwnedPreviewRegister, PlayState, PositionInBeats, PositionInSeconds, ReaperMutex,
    ReaperMutexGuard, ReaperVolumeValue,
};

use helgoboss_learn::{UnitValue, BASE_EPSILON};

use crate::domain::clip::clip_source::{
    ClipPcmSource, ClipPcmSourceSkills, ClipPcmSourceState, ClipStopPosition,
};
use crate::domain::clip::source_util::pcm_source_is_midi;
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
                Some(source.query_inner_length())
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
        self.state.play_state()
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
        new_play_state: PlayState,
    ) -> Result<Option<ClipChangedEvent>, &'static str> {
        if !self.clip.repeat {
            // One-shots should not be synchronized with main timeline.
            return Ok(None);
        }
        let result = self
            .start_transition()
            .process_transport_change(&self.register, new_play_state);
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
        let src = guard.src().ok_or("no source loaded")?;
        if !matches!(self.state, State::Playing(_)) {
            return Ok(UnitValue::MIN);
        }
        let src = src.as_ref();
        let pos_within_clip = src.pos_within_clip_scheduled();
        let length = src.query_inner_length();
        let percentage_pos = calculate_proportional_position(pos_within_clip, length);
        Ok(percentage_pos)
    }

    /// Returns the current clip position in seconds.
    pub fn position_in_seconds(&self) -> PositionInSeconds {
        lock(&self.register).cur_pos()
    }

    /// Changes the clip position on a percentage basis.
    pub fn set_position(&mut self, position: UnitValue) -> Result<ClipChangedEvent, &'static str> {
        let mut guard = lock(&self.register);
        let source = guard.src_mut().ok_or("no source loaded")?;
        let source = source.as_mut();
        let length = source.query_inner_length();
        let real_pos = PositionInSeconds::new(position.get() * length.get());
        guard.set_cur_pos(real_pos);
        Ok(ClipChangedEvent::ClipPosition(position))
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
///
/// This enum is essentially a state machine and the methods are functional transitions.  
#[derive(Debug)]
enum State {
    Empty,
    Suspended(SuspendedState),
    Playing(PlayingState),
    Transitioning,
}

impl State {
    /// Derives the corresponding clip play state.
    pub fn play_state(&self) -> ClipPlayState {
        use State::*;
        match self {
            Empty => ClipPlayState::Stopped,
            Suspended(s) => {
                if s.is_paused {
                    ClipPlayState::Paused
                } else {
                    ClipPlayState::Stopped
                }
            }
            Playing(s) => match s.scheduled_for {
                None => ClipPlayState::Playing,
                Some(ScheduledFor::Play) => ClipPlayState::ScheduledForPlay,
                Some(ScheduledFor::Stop) => ClipPlayState::ScheduledForStop,
            },
            Transitioning => unreachable!(),
        }
    }

    pub fn process_transport_change(
        self,
        reg: &SharedRegister,
        new_play_state: PlayState,
    ) -> TransitionResult {
        use State::*;
        match self {
            Suspended(s) if s.was_caused_by_transport_change => {
                if new_play_state.is_playing && !new_play_state.is_paused {
                    if let Some(play_args) = s.last_play_args.clone() {
                        if play_args.options.next_bar {
                            s.play(reg, play_args)
                        } else {
                            Ok(Suspended(s))
                        }
                    } else {
                        Ok(Suspended(s))
                    }
                } else {
                    Ok(Suspended(s))
                }
            }
            Playing(s) if s.args.options.next_bar => {
                if new_play_state.is_playing {
                    Ok(Playing(s))
                } else if new_play_state.is_paused {
                    s.pause(reg, true)
                } else {
                    s.stop_immediately(reg, true)
                }
            }
            s => Ok(s),
        }
    }

    pub fn play(self, reg: &SharedRegister, args: ClipPlayArgs) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((Empty, "slot is empty")),
            Suspended(s) => s.play(reg, args),
            Playing(s) => s.play(reg, args),
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
            Suspended(s) => s.stop(reg),
            Playing(s) => s.stop(reg, stop_behavior, false, project),
            Transitioning => unreachable!(),
        }
    }

    pub fn pause(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            s @ Empty | s @ Suspended(_) => Ok(s),
            Playing(s) => s.pause(reg, false),
            Transitioning => unreachable!(),
        }
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Suspended(s) => s.clear(reg),
            Playing(s) => s.clear(reg),
            Transitioning => unreachable!(),
        }
    }

    pub fn poll(self, reg: &SharedRegister) -> (TransitionResult, Option<ClipChangedEvent>) {
        use State::*;
        match self {
            Playing(s) => s.poll(reg),
            _ => (Ok(self), None),
        }
    }

    pub fn fill_with_source(self, source: OwnedSource, reg: &SharedRegister) -> TransitionResult {
        let source = ClipPcmSource::new(source.into_raw());
        let source = create_custom_owned_pcm_source(source);
        let source = FlexibleOwnedPcmSource::Custom(source);
        use State::*;
        match self {
            Empty | Suspended(_) => {
                let mut g = lock(reg);
                g.set_src(Some(source));
                // This only has an effect if "Next bar" disabled.
                g.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(Suspended(SuspendedState {
                    is_paused: false,
                    last_play_args: None,
                    was_caused_by_transport_change: false,
                }))
            }
            Playing(s) => s.fill_with_source(source, reg),
            Transitioning => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct SuspendedState {
    is_paused: bool,
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

impl SuspendedState {
    pub fn play(self, reg: &SharedRegister, args: ClipPlayArgs) -> TransitionResult {
        {
            let mut guard = lock(reg);
            guard.set_preview_track(args.track.as_ref().map(|t| t.raw()));
            if let Some(src) = guard.src_mut() {
                let scheduled_pos = if args.options.next_bar {
                    let scheduled_pos = get_next_bar_pos(args.project);
                    Some(scheduled_pos)
                } else {
                    None
                };
                src.as_mut().schedule_start(scheduled_pos, args.repeat);
            }
        }
        let buffering_behavior = BitFlags::empty();
        let measure_alignment = MeasureAlignment::PlayImmediately;
        let result = if let Some(track) = args.track.as_ref() {
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
        match result {
            Ok(handle) => {
                let scheduling_state = if args.options.next_bar {
                    Some(ScheduledFor::Play)
                } else {
                    None
                };
                let next_state = PlayingState {
                    handle,
                    args,
                    scheduled_for: scheduling_state,
                };
                Ok(State::Playing(next_state))
            }
            Err(_) => Err((State::Suspended(self), "couldn't play preview")),
        }
    }

    pub fn stop(self, reg: &SharedRegister) -> TransitionResult {
        let next_state = State::Suspended(self);
        let mut g = lock(reg);
        // Reset position. Only has an effect if "Next bar" disabled.
        g.set_cur_pos(PositionInSeconds::new(0.0));
        Ok(next_state)
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        let mut g = lock(reg);
        g.set_src(None);
        // Reset position. Only has an effect if "Next bar" disabled.
        g.set_cur_pos(PositionInSeconds::new(0.0));
        Ok(State::Empty)
    }
}

#[derive(Debug)]
struct PlayingState {
    handle: NonNull<raw::preview_register_t>,
    args: ClipPlayArgs,
    scheduled_for: Option<ScheduledFor>,
}

#[derive(Debug)]
enum ScheduledFor {
    /// Not yet playing but will soon. Final play detection done by polling.
    Play,
    /// Still playing but will stop soon. Final stop detection done by polling.
    Stop,
}

impl PlayingState {
    pub fn play(self, reg: &SharedRegister, args: ClipPlayArgs) -> TransitionResult {
        if self.args.track.as_ref() != args.track.as_ref() {
            // Track change!
            self.suspend(reg, true, false).play(reg, args)
        } else {
            // No track change.
            let next_state = match self.scheduled_for {
                None => {
                    // Retrigger!
                    // Previously we simply enqueued an "All notes off" sequence here and
                    // reset the position to zero. But with "Next bar" this would send "All notes
                    // off" not before starting playing again - causing some hanging notes in the
                    // meantime.
                    wait_until_all_notes_off_sent(reg, true);
                    let playing = PlayingState {
                        scheduled_for: if self.args.options.next_bar {
                            // REAPER won't start playing until the next bar starts.
                            Some(ScheduledFor::Play)
                        } else {
                            None
                        },
                        ..self
                    };
                    State::Playing(playing)
                }
                Some(ScheduledFor::Play) => {
                    // Nothing to do.
                    State::Playing(self)
                }
                Some(ScheduledFor::Stop) => {
                    // Backpedal (undo schedule for stop)!
                    let mut guard = lock(reg);
                    if let Some(src) = guard.src_mut() {
                        src.as_mut().backpedal_from_scheduled_stop();
                    }
                    State::Playing(PlayingState {
                        scheduled_for: None,
                        ..self
                    })
                }
            };
            Ok(next_state)
        }
    }

    pub fn fill_with_source(
        self,
        source: FlexibleOwnedPcmSource,
        reg: &SharedRegister,
    ) -> TransitionResult {
        let mut g = lock(reg);
        g.set_src(Some(source));
        Ok(State::Playing(self))
    }

    pub fn stop(
        self,
        reg: &SharedRegister,
        stop_behavior: SlotStopBehavior,
        caused_by_transport_change: bool,
        project: Project,
    ) -> TransitionResult {
        use SlotStopBehavior::*;
        match stop_behavior {
            Immediately => self.stop_immediately(reg, caused_by_transport_change),
            EndOfBar | EndOfClip => {
                match self.scheduled_for {
                    None => {
                        // Schedule stop.
                        let mut guard = lock(reg);
                        if let Some(src) = guard.src_mut() {
                            let scheduled_pos = self.get_clip_stop_position(stop_behavior, project);
                            src.as_mut().schedule_stop(scheduled_pos)
                        }
                        let playing = PlayingState {
                            scheduled_for: Some(ScheduledFor::Stop),
                            ..self
                        };
                        Ok(State::Playing(playing))
                    }
                    Some(ScheduledFor::Play) => {
                        // We haven't even started playing yet! Okay, let's backpedal.
                        // This is currently not reachable in "Toggle button" mode because we consider
                        // "Scheduled for play" as 25% which is from the perspective of toggle mode
                        // still "off". So it will only send an "on" signal.
                        let suspended = self.suspend(reg, false, caused_by_transport_change);
                        Ok(State::Suspended(suspended))
                    }
                    Some(ScheduledFor::Stop) => {
                        // We are scheduled for stop already. Take that as a request for immediate
                        // stop.
                        let suspended =
                            self.stop_immediately_internal(reg, caused_by_transport_change);
                        Ok(State::Suspended(suspended))
                    }
                }
            }
        }
    }

    fn stop_immediately(
        self,
        reg: &SharedRegister,
        caused_by_transport_change: bool,
    ) -> TransitionResult {
        let suspended = self.stop_immediately_internal(reg, caused_by_transport_change);
        Ok(State::Suspended(suspended))
    }

    fn get_clip_stop_position(
        &self,
        stop_behavior: SlotStopBehavior,
        project: Project,
    ) -> ClipStopPosition {
        use SlotStopBehavior::*;
        match stop_behavior {
            EndOfBar => ClipStopPosition::At(get_next_bar_pos(project)),
            EndOfClip => ClipStopPosition::AtEndOfClip,
            Immediately => unimplemented!("not used"),
        }
    }

    fn stop_immediately_internal(
        self,
        reg: &SharedRegister,
        caused_by_transport_change: bool,
    ) -> SuspendedState {
        let suspended = self.suspend(reg, false, caused_by_transport_change);
        let mut g = lock(reg);
        // Reset position! Only has an effect if "Next bar" disabled.
        // TODO-medium I think setting the cursor position of the preview register is not even
        //  necessary anymore because we don't use it, or do we?
        g.set_cur_pos(PositionInSeconds::new(0.0));
        suspended
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        self.suspend(reg, false, false).clear(reg)
    }

    pub fn pause(self, reg: &SharedRegister, caused_by_transport_change: bool) -> TransitionResult {
        Ok(State::Suspended(self.suspend(
            reg,
            true,
            caused_by_transport_change,
        )))
    }

    pub fn poll(self, reg: &SharedRegister) -> (TransitionResult, Option<ClipChangedEvent>) {
        let (pos_within_clip, length) = {
            // React gracefully even in weird situations (because we are in poll).
            let guard = match reg.lock() {
                Ok(g) => g,
                Err(_) => return (Ok(State::Playing(self)), None),
            };
            let src = match guard.src() {
                Some(s) => s,
                None => return (Ok(State::Playing(self)), None),
            };
            let src = src.as_ref();
            let pos_within_clip = src.pos_within_clip_scheduled();
            let length = src.query_inner_length();
            (pos_within_clip, length)
        };
        let (next_state, event) = match self.scheduled_for {
            None | Some(ScheduledFor::Stop) => {
                // Playing normally or scheduled for stop.
                if pos_within_clip.is_some() {
                    // Still playing
                    (Ok(State::Playing(self)), None)
                } else {
                    // Not playing anymore. Make it official! If we let the preview running,
                    // nothing will happen because it's not looped but the preview will still be
                    // active (e.g. respond to position changes) - which can't be good.
                    (
                        self.stop_immediately(reg, false),
                        Some(ClipChangedEvent::PlayState(ClipPlayState::Stopped)),
                    )
                }
            }
            Some(ScheduledFor::Play) => {
                if let Some(p) = pos_within_clip {
                    if p < PositionInSeconds::ZERO {
                        // Still counting in.
                        (Ok(State::Playing(self)), None)
                    } else {
                        // Actual play detected. Make it official.
                        let next_playing_state = PlayingState {
                            scheduled_for: None,
                            ..self
                        };
                        (
                            Ok(State::Playing(next_playing_state)),
                            Some(ClipChangedEvent::PlayState(ClipPlayState::Playing)),
                        )
                    }
                } else {
                    // Probably length zero.
                    (
                        self.stop_immediately(reg, false),
                        Some(ClipChangedEvent::PlayState(ClipPlayState::Stopped)),
                    )
                }
            }
        };
        let final_event = event.unwrap_or_else(|| {
            let position = calculate_proportional_position(pos_within_clip, length);
            ClipChangedEvent::ClipPosition(position)
        });
        (next_state, Some(final_event))
    }

    fn suspend(
        self,
        reg: &SharedRegister,
        pause: bool,
        caused_by_transport_change: bool,
    ) -> SuspendedState {
        // TODO-high Now that we control the source itself, we could do this differently!
        wait_until_all_notes_off_sent(reg, false);
        if let Some(track) = self.args.track.as_ref() {
            // Check prevents error message on project close.
            let project = track.project();
            if project.is_available() {
                let _ = Reaper::get()
                    .medium_session()
                    .stop_track_preview_2(project.context(), self.handle);
            }
        } else {
            // If not successful this probably means it was stopped already, so okay.
            let _ = Reaper::get().medium_session().stop_preview(self.handle);
        };
        SuspendedState {
            is_paused: pause,
            last_play_args: Some(self.args),
            was_caused_by_transport_change: caused_by_transport_change,
        }
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

/// Waits until "all-notes-off" is sent for MIDI clips, e.g. as preparation for a suspension
/// request.
fn wait_until_all_notes_off_sent(reg: &SharedRegister, reset_position: bool) {
    // Try 10 times
    for _ in 0..10 {
        if attempt_to_send_all_notes_off(reg, reset_position) {
            // Preparation finished.
            return;
        }
        // Wait a tiny bit until the next try
        std::thread::sleep(Duration::from_millis(5));
    }
    // If we make it until here, we tried multiple times without success.
    // Make sure source gets reset to normal.
    let mut guard = lock(reg);
    if reset_position {
        guard.set_cur_pos(PositionInSeconds::new(0.0));
    }
    let src = match guard.src_mut() {
        None => return,
        Some(s) => s.as_mut(),
    };
    src.reset();
}

/// Returns `true` as soon as "All notes off" sent.
fn attempt_to_send_all_notes_off(reg: &SharedRegister, reset_position: bool) -> bool {
    let mut guard = lock(reg);
    let successfully_sent = attempt_to_send_all_notes_off_with_guard(&mut guard);
    if successfully_sent && reset_position {
        guard.set_cur_pos(PositionInSeconds::new(0.0));
    };
    successfully_sent
}

/// Returns `true` as soon as "All notes off" sent.
fn attempt_to_send_all_notes_off_with_guard(
    guard: &mut ReaperMutexGuard<OwnedPreviewRegister>,
) -> bool {
    let src = match guard.src_mut() {
        None => return true,
        Some(s) => s,
    };
    let src = src.as_mut();
    if !pcm_source_is_midi(src) {
        return true;
    }
    // Don't just stop MIDI! Send all-notes-off first to prevent hanging notes.
    use ClipPcmSourceState::*;
    match src.query_state() {
        Normal => {
            src.request_all_notes_off();
            false
        }
        AllNotesOffRequested => {
            // Wait
            false
        }
        AllNotesOffSent => {
            src.reset();
            true
        }
    }
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
