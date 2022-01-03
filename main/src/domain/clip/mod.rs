use std::convert::TryInto;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};
use std::sync::Arc;
use std::time::Duration;

use enumflags2::BitFlags;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{Item, OwnedSource, Project, Reaper, ReaperSource, Track};
use reaper_low::raw;
use reaper_medium::{
    create_custom_owned_pcm_source, reaper_str, BorrowedPcmSource, BorrowedPcmSourceTransfer,
    BufferingBehavior, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs,
    FlexibleOwnedPcmSource, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, MeasureAlignment,
    MeasureMode, MidiEvent, MidiImportBehavior, OwnedPcmSource, OwnedPreviewRegister, PcmSource,
    PeaksClearArgs, PlayState, PositionInBeats, PositionInSeconds, ProjectContext,
    PropertiesWindowArgs, ReaperMutex, ReaperMutexGuard, ReaperStr, ReaperVolumeValue,
    SaveStateArgs, SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
};
use serde::{Deserialize, Serialize};

use helgoboss_learn::{UnitValue, BASE_EPSILON};

use crate::base::default_util::is_default;
use crate::domain::ClipChangedEvent;

mod clip;
pub use clip::*;

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
        track: Option<Track>,
        options: SlotPlayOptions,
    ) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().play(
            &self.register,
            ClipPlayArgs {
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
    pub fn stop(&mut self, immediately: bool) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().stop(&self.register, immediately);
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
        // TODO-high Return based on guard.cur_pos() if slot is not synced (make this a slot prop!).
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
        let mut source = guard.src_mut().ok_or("no source loaded")?;
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
                    s.stop(reg, true, true)
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

    pub fn stop(self, reg: &SharedRegister, immediately: bool) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Suspended(s) => s.stop(reg),
            Playing(s) => s.stop(reg, immediately, false),
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
        let source = WrapperPcmSource::new(source.into_raw());
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
                let mut scheduled_pos = if args.options.next_bar {
                    let scheduled_pos = get_next_bar_pos();
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
        immediately: bool,
        caused_by_transport_change: bool,
    ) -> TransitionResult {
        if immediately {
            let suspended = self.stop_immediately(reg, caused_by_transport_change);
            Ok(State::Suspended(suspended))
        } else {
            match self.scheduled_for {
                None => {
                    // Schedule stop.
                    let mut guard = lock(reg);
                    if let Some(src) = guard.src_mut() {
                        let scheduled_pos = get_next_bar_pos();
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
                    let suspended = self.stop_immediately(reg, caused_by_transport_change);
                    Ok(State::Suspended(suspended))
                }
            }
        }
    }

    fn stop_immediately(
        self,
        reg: &SharedRegister,
        caused_by_transport_change: bool,
    ) -> SuspendedState {
        let suspended = self.suspend(reg, false, caused_by_transport_change);
        let mut g = lock(reg);
        // Reset position! Only has an effect if "Next bar" disabled.
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
            // TODO-high Return based on guard.cur_pos() if slot is not synced (make this a slot
            // prop!).
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
                        self.stop(reg, true, false),
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
                        self.stop(reg, true, false),
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

const EXT_REQUEST_ALL_NOTES_OFF: i32 = 2359767;
const EXT_QUERY_STATE: i32 = 2359769;
const EXT_RESET: i32 = 2359770;
const EXT_SCHEDULE_START: i32 = 2359771;
const EXT_QUERY_INNER_LENGTH: i32 = 2359772;
const EXT_ENABLE_REPEAT: i32 = 2359773;
const EXT_DISABLE_REPEAT: i32 = 2359774;
const EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED: i32 = 2359775;
const EXT_SCHEDULE_STOP: i32 = 2359776;
const EXT_BACKPEDAL_FROM_SCHEDULED_STOP: i32 = 2359777;

/// Represents a state of the clip wrapper PCM source.
#[derive(Copy, Clone, Eq, PartialEq, Debug, TryFromPrimitive, IntoPrimitive)]
#[repr(i32)]
enum WrapperPcmSourceState {
    Normal = 10,
    AllNotesOffRequested = 11,
    AllNotesOffSent = 12,
}

/// A PCM source which wraps a native REAPER PCM source and applies all kinds of clip
/// functionality to it.
///
/// For example, it makes sure it starts at the right position.
struct WrapperPcmSource {
    inner: OwnedPcmSource,
    state: WrapperPcmSourceState,
    counter: u64,
    scheduled_start_pos: Option<PositionInSeconds>,
    scheduled_stop_pos: Option<PositionInSeconds>,
    repeated: bool,
    is_midi: bool,
}

impl WrapperPcmSource {
    /// Wraps the given native REAPER PCM source.
    pub fn new(inner: OwnedPcmSource) -> Self {
        let is_midi = pcm_source_is_midi(&inner);
        Self {
            inner,
            state: WrapperPcmSourceState::Normal,
            counter: 0,
            scheduled_start_pos: None,
            scheduled_stop_pos: None,
            repeated: false,
            is_midi,
        }
    }

    /// Returns the position starting from the time that this source was scheduled for start.
    ///
    /// Returns `None` if not scheduled or if beyond scheduled stop. Returns negative position if
    /// clip not yet playing.
    fn pos_from_start_scheduled(&self) -> Option<PositionInSeconds> {
        let scheduled_start_pos = self.scheduled_start_pos?;
        // Position in clip is synced to project position.
        let project_pos = Reaper::get()
            .medium_reaper()
            // TODO-high Save actual project in source.
            .get_play_position_2_ex(ProjectContext::CurrentProject);
        // Return `None` if scheduled stop position is reached.
        if let Some(scheduled_stop_pos) = self.scheduled_stop_pos {
            if project_pos.has_reached(scheduled_stop_pos) {
                return None;
            }
        }
        Some(project_pos - scheduled_start_pos)
    }

    /// Calculates the current position within the clip considering the *repeated* setting.
    ///
    /// Returns negative position if clip not yet playing. Returns `None` if clip length is zero.
    fn calculate_pos_within_clip(
        &self,
        pos_from_start: PositionInSeconds,
    ) -> Option<PositionInSeconds> {
        if pos_from_start < PositionInSeconds::ZERO {
            // Count-in phase. Report negative position.
            Some(pos_from_start)
        } else if self.repeated {
            // Playing and repeating. Report repeated position.
            pos_from_start % self.query_inner_length()
        } else if pos_from_start.get() < self.query_inner_length().get() {
            // Not repeating and still within clip bounds. Just report position.
            Some(pos_from_start)
        } else {
            // Not repeating and clip length exceeded.
            None
        }
    }
}

impl CustomPcmSource for WrapperPcmSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        // Not correct but probably never used.
        self.inner.duplicate()
    }

    fn is_available(&mut self) -> bool {
        self.inner.is_available()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        self.inner.set_available(args.is_available);
    }

    fn get_type(&mut self) -> &ReaperStr {
        unsafe { self.inner.get_type_unchecked() }
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unsafe { self.inner.get_file_name_unchecked() }
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        self.inner.set_file_name(args.new_file_name)
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        self.inner.get_source()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        self.inner.set_source(args.source);
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.inner.get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        self.inner.get_sample_rate()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        DurationInSeconds::MAX
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        let _ = self.inner.get_length_beats()?;
        Some(DurationInBeats::MAX)
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        self.inner.get_bits_per_sample()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        self.inner.get_preferred_position()
    }

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unsafe { self.inner.properties_window(args.parent_window) }
    }

    fn get_samples(&mut self, args: GetSamplesArgs) {
        // Debugging
        // if self.counter % 500 == 0 {
        //     let ptr = args.block.as_ptr();
        //     let raw = unsafe { ptr.as_ref() };
        //     dbg!(raw);
        // }
        self.counter += 1;
        // Actual stuff
        use WrapperPcmSourceState::*;
        match self.state {
            Normal => unsafe {
                // TODO-high If not synced, do something like this:
                // let pos_from_start = args.block.time_s();
                // self.calculate_pos_within_clip(pos_from_start)
                if let Some(pos) = self.pos_within_clip_scheduled() {
                    // We want to start playing as soon as we reach the scheduled start position,
                    // that means pos == 0.0. In order to do that, we need to take into account that
                    // the audio buffer start point is not necessarily equal to the measure start
                    // point. If we would naively start playing as soon as pos >= 0.0, we might skip
                    // the first samples/messages! We need to start playing as soon as the end of
                    // the audio block is located on or right to the scheduled start point
                    // (end_pos >= 0.0).
                    let desired_sample_count = args.block.length();
                    let sample_rate = args.block.sample_rate().get();
                    let block_duration = desired_sample_count as f64 / sample_rate;
                    let end_pos =
                        unsafe { PositionInSeconds::new_unchecked(pos.get() + block_duration) };
                    if end_pos < PositionInSeconds::ZERO {
                        return;
                    }
                    if self.is_midi {
                        // MIDI.
                        // For MIDI it seems to be okay to start at a negative position. The source
                        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
                        // offset.
                        args.block.set_time_s(pos);
                        self.inner.get_samples(args.block);
                        let written_sample_count = args.block.samples_out();
                        if written_sample_count < desired_sample_count {
                            // We have reached the end of the clip and it doesn't fill the
                            // complete block.
                            if self.repeated {
                                // Repeat. Fill rest of buffer with beginning of source.
                                // We need to start from negative position so the frame
                                // offset of the *added* MIDI events is correctly written.
                                // The negative position should be as long as the duration of
                                // samples already written.
                                let written_duration = written_sample_count as f64 / sample_rate;
                                let negative_pos =
                                    unsafe { PositionInSeconds::new_unchecked(-written_duration) };
                                args.block.set_time_s(negative_pos);
                                args.block.set_length(desired_sample_count);
                                self.inner.get_samples(args.block);
                            } else {
                                // Let preview register know that complete buffer has been
                                // filled as desired in order to prevent retry (?) queries that
                                // lead to double events.
                                args.block.set_samples_out(desired_sample_count);
                            }
                        }
                    } else {
                        // Audio.
                        if pos < PositionInSeconds::ZERO {
                            // For audio, starting at a negative position leads to weird sounds.
                            // That's why we need to query from 0.0 and
                            // offset the provided sample buffer by that
                            // amount.
                            let sample_offset = (-pos.get() * sample_rate) as i32;
                            args.block.set_time_s(PositionInSeconds::ZERO);
                            with_shifted_samples(args.block, sample_offset, |b| {
                                self.inner.get_samples(b);
                            });
                        } else {
                            args.block.set_time_s(pos);
                            self.inner.get_samples(args.block);
                        }
                        let written_sample_count = args.block.samples_out();
                        if written_sample_count < desired_sample_count {
                            // We have reached the end of the clip and it doesn't fill the
                            // complete block.
                            if self.repeated {
                                // Repeat. Because we assume that the user cuts sources
                                // sample-perfect, we must immediately fill the rest of the
                                // buffer with the very
                                // beginning of the source.
                                // Audio. Start from zero and write just remaining samples.
                                args.block.set_time_s(PositionInSeconds::ZERO);
                                with_shifted_samples(args.block, written_sample_count, |b| {
                                    self.inner.get_samples(b);
                                });
                                // Let preview register know that complete buffer has been filled.
                                args.block.set_samples_out(desired_sample_count);
                            } else {
                                // Let preview register know that complete buffer has been
                                // filled as desired in order to prevent retry (?) queries.
                                args.block.set_samples_out(desired_sample_count);
                            }
                        }
                    }
                }
            },
            AllNotesOffRequested => {
                send_all_notes_off(&args);
                self.state = AllNotesOffSent;
            }
            AllNotesOffSent => {}
        }
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unsafe {
            self.inner.get_peak_info(args.block);
        }
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unsafe {
            self.inner.save_state(args.context);
        }
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unsafe { self.inner.load_state(args.first_line, args.context) }
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
        self.inner.peaks_clear(args.delete_file);
    }

    fn peaks_build_begin(&mut self) -> bool {
        self.inner.peaks_build_begin()
    }

    fn peaks_build_run(&mut self) -> bool {
        self.inner.peaks_build_run()
    }

    fn peaks_build_finish(&mut self) {
        self.inner.peaks_build_finish();
    }

    unsafe fn extended(&mut self, args: ExtendedArgs) -> i32 {
        match args.call {
            EXT_REQUEST_ALL_NOTES_OFF => {
                self.request_all_notes_off();
                1
            }
            EXT_QUERY_STATE => self.query_state().into(),
            EXT_RESET => {
                self.reset();
                1
            }
            EXT_SCHEDULE_START => {
                let pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let pos = if pos.get() < 0.0 { None } else { Some(pos) };
                let repeated: bool = *(args.parm_2 as *mut _);
                self.schedule_start(pos, repeated);
                1
            }
            EXT_SCHEDULE_STOP => {
                let pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.schedule_stop(pos);
                1
            }
            EXT_BACKPEDAL_FROM_SCHEDULED_STOP => {
                self.backpedal_from_scheduled_stop();
                1
            }
            EXT_QUERY_INNER_LENGTH => {
                *(args.parm_1 as *mut f64) = self.query_inner_length().get();
                1
            }
            EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED => {
                *(args.parm_1 as *mut f64) = if let Some(pos) = self.pos_within_clip_scheduled() {
                    pos.get()
                } else {
                    f64::NAN
                };
                1
            }
            EXT_ENABLE_REPEAT => {
                self.set_repeated(true);
                1
            }
            EXT_DISABLE_REPEAT => {
                self.set_repeated(false);
                1
            }
            _ => self
                .inner
                .extended(args.call, args.parm_1, args.parm_2, args.parm_3),
        }
    }
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
    mut guard: &mut ReaperMutexGuard<OwnedPreviewRegister>,
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
    use WrapperPcmSourceState::*;
    match src.query_state() {
        Normal => unsafe {
            src.request_all_notes_off();
            false
        },
        AllNotesOffRequested => {
            // Wait
            false
        }
        AllNotesOffSent => unsafe {
            src.reset();
            true
        },
    }
}

fn send_all_notes_off(args: &GetSamplesArgs) {
    for ch in 0..16 {
        let msg = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_NOTES_OFF,
            U7::MIN,
        );
        let mut event = MidiEvent::default();
        event.set_message(msg);
        args.block.midi_event_list().add_item(&event);
    }
}

trait WrapperPcmSourceSkills {
    fn request_all_notes_off(&mut self);
    fn query_state(&self) -> WrapperPcmSourceState;
    fn reset(&mut self);
    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, repeated: bool);
    fn schedule_stop(&mut self, pos: PositionInSeconds);
    fn backpedal_from_scheduled_stop(&mut self);
    fn query_inner_length(&self) -> DurationInSeconds;
    fn set_repeated(&mut self, repeated: bool);
    ///
    /// - Considers repeat.
    /// - Returns negative position if clip not yet playing.
    /// - Returns `None` if not scheduled, if beyond scheduled stop or if clip length is zero.
    /// - Returns (hypothetical) position even if not playing!
    fn pos_within_clip_scheduled(&self) -> Option<PositionInSeconds>;
}

impl WrapperPcmSourceSkills for WrapperPcmSource {
    fn request_all_notes_off(&mut self) {
        self.state = WrapperPcmSourceState::AllNotesOffRequested;
    }

    fn query_state(&self) -> WrapperPcmSourceState {
        self.state
    }

    fn reset(&mut self) {
        self.state = WrapperPcmSourceState::Normal;
    }

    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, repeated: bool) {
        self.scheduled_start_pos = pos;
        self.scheduled_stop_pos = None;
        self.repeated = repeated;
    }

    fn schedule_stop(&mut self, pos: PositionInSeconds) {
        self.scheduled_stop_pos = Some(pos);
    }

    fn backpedal_from_scheduled_stop(&mut self) {
        self.scheduled_stop_pos = None;
    }

    fn query_inner_length(&self) -> DurationInSeconds {
        self.inner.get_length().unwrap_or_default()
    }

    fn set_repeated(&mut self, repeated: bool) {
        self.repeated = repeated;
    }

    fn pos_within_clip_scheduled(&self) -> Option<PositionInSeconds> {
        let pos_from_start = self.pos_from_start_scheduled()?;
        self.calculate_pos_within_clip(pos_from_start)
    }
}

impl WrapperPcmSourceSkills for BorrowedPcmSource {
    fn request_all_notes_off(&mut self) {
        unsafe {
            self.extended(
                EXT_REQUEST_ALL_NOTES_OFF,
                null_mut(),
                null_mut(),
                null_mut(),
            );
        }
    }

    fn query_state(&self) -> WrapperPcmSourceState {
        let state = unsafe { self.extended(EXT_QUERY_STATE, null_mut(), null_mut(), null_mut()) };
        state.try_into().expect("invalid state")
    }

    fn reset(&mut self) {
        unsafe {
            self.extended(EXT_RESET, null_mut(), null_mut(), null_mut());
        }
    }

    fn schedule_start(&mut self, pos: Option<PositionInSeconds>, mut repeated: bool) {
        let mut raw_pos = if let Some(p) = pos { p.get() } else { -1.0 };
        unsafe {
            self.extended(
                EXT_SCHEDULE_START,
                &mut raw_pos as *mut _ as _,
                &mut repeated as *mut _ as _,
                null_mut(),
            );
        }
    }

    fn schedule_stop(&mut self, pos: PositionInSeconds) {
        let mut raw_pos = pos.get();
        unsafe {
            self.extended(
                EXT_SCHEDULE_STOP,
                &mut raw_pos as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn backpedal_from_scheduled_stop(&mut self) {
        unsafe {
            self.extended(
                EXT_BACKPEDAL_FROM_SCHEDULED_STOP,
                null_mut(),
                null_mut(),
                null_mut(),
            );
        }
    }

    fn query_inner_length(&self) -> DurationInSeconds {
        let mut l = 0.0;
        unsafe {
            self.extended(
                EXT_QUERY_INNER_LENGTH,
                &mut l as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        DurationInSeconds::new(l)
    }

    fn set_repeated(&mut self, repeated: bool) {
        let request = if repeated {
            EXT_ENABLE_REPEAT
        } else {
            EXT_DISABLE_REPEAT
        };
        unsafe {
            self.extended(request, null_mut(), null_mut(), null_mut());
        }
    }

    fn pos_within_clip_scheduled(&self) -> Option<PositionInSeconds> {
        let mut p = f64::NAN;
        unsafe {
            self.extended(
                EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED,
                &mut p as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        if p.is_nan() {
            return None;
        }
        Some(PositionInSeconds::new(p))
    }
}

trait PositionInSecondsExt {
    fn has_reached(self, stop_pos: PositionInSeconds) -> bool;
}

impl PositionInSecondsExt for PositionInSeconds {
    fn has_reached(self, stop_pos: PositionInSeconds) -> bool {
        self > stop_pos || (stop_pos - self).get() < BASE_EPSILON
    }
}

fn get_next_bar_pos() -> PositionInSeconds {
    // TODO-high Use actual project.
    let reaper = Reaper::get().medium_reaper();
    let proj_context = ProjectContext::CurrentProject;
    let current_pos = reaper.get_play_position_2_ex(proj_context);
    let res = reaper.time_map_2_time_to_beats(proj_context, current_pos);
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

fn pcm_source_is_midi(src: &BorrowedPcmSource) -> bool {
    src.get_type(|t| t == reaper_str!("MIDI"))
}

unsafe fn with_shifted_samples(
    block: &mut BorrowedPcmSourceTransfer,
    offset: i32,
    f: impl FnOnce(&mut BorrowedPcmSourceTransfer),
) {
    // Shift samples.
    let original_length = block.length();
    let original_samples = block.samples();
    let shifted_samples = original_samples.offset((offset * block.nch()) as _);
    block.set_length(block.length() - offset);
    block.set_samples(shifted_samples);
    // Query inner source.
    f(block);
    // Unshift samples.
    block.set_length(original_length);
    block.set_samples(original_samples);
}
