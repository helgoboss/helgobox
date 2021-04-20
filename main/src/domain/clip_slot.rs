use crate::core::default_util::is_default;
use crate::domain::ClipChangedEvent;
use enumflags2::BitFlags;
use helgoboss_learn::UnitValue;
use reaper_high::{Guid, Item, OwnedSource, Project, Reaper, Source, Take, Track};
use reaper_low::raw;
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    BufferingBehavior, DurationInSeconds, ExtGetPooledMidiIdResult, MeasureAlignment, MediaItem,
    MidiImportBehavior, OwnedPreviewRegister, PcmSource, PositionInSeconds, ProjectContext,
    ReaperFunctionError, ReaperLockError, ReaperMutex, ReaperMutexGuard, ReaperVolumeValue,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::mem;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::ptr::{null_mut, NonNull};
use std::sync::Arc;

type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SlotDescriptor {
    #[serde(rename = "volume", default, skip_serializing_if = "is_default")]
    pub volume: ReaperVolumeValue,
    #[serde(rename = "repeat", default, skip_serializing_if = "is_default")]
    pub repeat: bool,
    #[serde(rename = "content", default, skip_serializing_if = "is_default")]
    pub content: Option<SlotContent>,
}

impl Default for SlotDescriptor {
    fn default() -> Self {
        Self {
            volume: ReaperVolumeValue::ZERO_DB,
            repeat: false,
            content: None,
        }
    }
}

impl SlotDescriptor {
    pub fn is_filled(&self) -> bool {
        self.content.is_some()
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SlotContent {
    File {
        #[serde(rename = "file")]
        file: PathBuf,
    },
}

impl SlotContent {
    pub fn file(&self) -> Option<&Path> {
        use SlotContent::*;
        match self {
            File { file } => Some(file),
        }
    }

    pub fn create_source(&self, project: Option<Project>) -> Result<OwnedSource, &'static str> {
        match self {
            SlotContent::File { file } => {
                let absolute_file = if file.is_relative() {
                    project
                        .ok_or("slot source given as relative file but without project")?
                        .make_path_absolute(file)
                        .ok_or("couldn't make clip source path absolute")?
                } else {
                    file.clone()
                };
                OwnedSource::from_file(&absolute_file, MidiImportBehavior::UsePreference)
            }
        }
    }
}

#[derive(Debug)]
pub struct ClipSlot {
    descriptor: SlotDescriptor,
    register: SharedRegister,
    state: State,
}

impl Default for ClipSlot {
    fn default() -> Self {
        let descriptor = SlotDescriptor::default();
        let register = create_shared_register(&descriptor);
        Self {
            descriptor,
            register,
            state: State::Empty,
        }
    }
}

fn create_shared_register(descriptor: &SlotDescriptor) -> SharedRegister {
    let mut register = OwnedPreviewRegister::default();
    register.set_volume(descriptor.volume);
    register.set_out_chan(-1);
    Arc::new(ReaperMutex::new(register))
}

impl ClipSlot {
    pub fn descriptor(&self) -> &SlotDescriptor {
        &self.descriptor
    }

    /// Resets all slot data to the defaults (including volume, repeat etc.).
    pub fn reset(&mut self) -> Result<Vec<ClipChangedEvent>, &'static str> {
        self.load(Default::default(), None)
    }

    /// Stops playback if necessary and loads all slot settings including the contained clip from
    /// the given descriptor.
    pub fn load(
        &mut self,
        descriptor: SlotDescriptor,
        project: Option<Project>,
    ) -> Result<Vec<ClipChangedEvent>, &'static str> {
        self.clear()?;
        // Using a completely new register saves us from cleaning up.
        self.register = create_shared_register(&descriptor);
        self.descriptor = descriptor;
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
        let source = if let Some(content) = self.descriptor.content.as_ref() {
            content.create_source(project)?
        } else {
            // Nothing to load
            return Ok(());
        };
        self.fill_with_source(source)?;
        Ok(())
    }

    pub fn fill_with_source_from_item(&mut self, item: Item) -> Result<(), Box<dyn Error>> {
        let active_take = item.active_take().ok_or("item has no active take")?;
        let root_source = active_take
            .source()
            .ok_or("take has no source")?
            .root_source();
        let source_type = root_source.r#type();
        let item_project = item.project();
        let file = if let Some(source_file) = root_source.file_name() {
            source_file
        } else if source_type == "MIDI" {
            let project = item_project.unwrap_or_else(|| Reaper::get().current_project());
            let recording_path = project.recording_path();
            let take_name = active_take.name();
            let take_name_slug = slug::slugify(take_name);
            let unique_id = nanoid::nanoid!(8);
            let file_name = format!("{}-{}.mid", take_name_slug, unique_id);
            let source_file = recording_path.join(file_name);
            root_source
                .export_to_file(&source_file)
                .map_err(|_| "couldn't export MIDI source to file")?;
            source_file
        } else {
            Err(format!("item source incompatible (type {})", source_type))?
        };
        let content = SlotContent::File {
            file: item_project
                .and_then(|p| p.make_path_relative_if_in_project_directory(&file))
                .unwrap_or(file),
        };
        self.fill_by_user(content, item_project)?;
        Ok(())
    }

    pub fn fill_by_user(
        &mut self,
        content: SlotContent,
        project: Option<Project>,
    ) -> Result<(), &'static str> {
        let source = content.create_source(project)?;
        self.fill_with_source(source)?;
        // Here it's important to not set the descriptor (change things) unless load was successful.
        self.descriptor.content = Some(content);
        Ok(())
    }

    pub fn clip_info(&self) -> Option<ClipInfo> {
        let source = self.state.source()?;
        let guard = self.register.lock().ok()?;
        let info = ClipInfo {
            r#type: source.r#type(),
            file_name: source.file_name(),
            length: source.length(),
        };
        // TODO-medium This is probably necessary to make sure the mutex is not unlocked before the
        //  PCM source operations are done. How can we solve this in a better way API-wise? On the
        //  other hand, we are on our own anyway when it comes to PCM source thread safety ...
        std::mem::drop(guard);
        Some(info)
    }

    /// Should be called regularly to detect stops.
    pub fn poll(&mut self) -> Option<ClipChangedEvent> {
        if self.play_state() != ClipPlayState::Playing {
            return None;
        }
        let (current_pos, length, is_looped) = {
            let guard = self.register.lock().ok()?;
            let source = guard.src()?;
            let length = unsafe { source.get_length() };
            (guard.cur_pos(), length, guard.is_looped())
        };
        match length {
            Some(l) if !is_looped && current_pos.get() >= l.get() => {
                self.stop(true).ok()?;
                Some(ClipChangedEvent::PlayStateChanged(ClipPlayState::Stopped))
            }
            _ => {
                let position = calculate_proportional_position(current_pos, length);
                Some(ClipChangedEvent::ClipPositionChanged(position))
            }
        }
    }

    pub fn is_filled(&self) -> bool {
        self.descriptor.is_filled()
    }

    pub fn source_is_loaded(&self) -> bool {
        !matches!(self.state, State::Empty)
    }

    pub fn play_state(&self) -> ClipPlayState {
        use State::*;
        match &self.state {
            Empty => ClipPlayState::Stopped,
            Suspended(s) => {
                if s.is_paused {
                    ClipPlayState::Paused
                } else {
                    ClipPlayState::Stopped
                }
            }
            Playing(_) => ClipPlayState::Playing,
            Transitioning => unreachable!(),
        }
    }

    pub fn play_state_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::PlayStateChanged(self.play_state())
    }

    fn fill_with_source(&mut self, source: OwnedSource) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .fill_with_source(source, &self.register);
        self.finish_transition(result)
    }

    pub fn play(
        &mut self,
        track: Option<&Track>,
        options: SlotPlayOptions,
    ) -> Result<ClipChangedEvent, &'static str> {
        let result =
            self.start_transition()
                .play(&self.register, options, track, self.descriptor.repeat);
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
    }

    /// Stops playback if necessary, destroys the contained source and resets the playback position
    /// to zero.
    pub fn clear(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().clear(&self.register);
        self.finish_transition(result)
    }

    pub fn stop(&mut self, immediately: bool) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().stop(&self.register, immediately);
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
    }

    pub fn pause(&mut self) -> Result<ClipChangedEvent, &'static str> {
        let result = self.start_transition().pause();
        self.finish_transition(result)?;
        Ok(self.play_state_changed_event())
    }

    pub fn repeat_is_enabled(&self) -> bool {
        self.descriptor.repeat
    }

    pub fn repeat_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::ClipRepeatChanged(self.descriptor.repeat)
    }

    pub fn toggle_repeat(&mut self) -> ClipChangedEvent {
        let new_value = !self.descriptor.repeat;
        self.descriptor.repeat = new_value;
        lock(&self.register).set_looped(new_value);
        self.repeat_changed_event()
    }

    pub fn volume(&self) -> ReaperVolumeValue {
        self.descriptor.volume
    }

    pub fn volume_changed_event(&self) -> ClipChangedEvent {
        ClipChangedEvent::ClipVolumeChanged(self.descriptor.volume)
    }

    pub fn set_volume(&mut self, volume: ReaperVolumeValue) -> ClipChangedEvent {
        self.descriptor.volume = volume;
        lock(&self.register).set_volume(volume);
        self.volume_changed_event()
    }

    pub fn position(&self) -> Result<UnitValue, &'static str> {
        let mut guard = lock(&self.register);
        let source = guard.src().ok_or("no source loaded")?;
        let length = unsafe { source.get_length() };
        let position = calculate_proportional_position(guard.cur_pos(), length);
        Ok(position)
    }

    pub fn set_position(&mut self, position: UnitValue) -> Result<ClipChangedEvent, &'static str> {
        let mut guard = lock(&self.register);
        let source = guard.src().ok_or("no source loaded")?;
        let length = unsafe { source.get_length().ok_or("source has no length")? };
        let real_pos = PositionInSeconds::new(position.get() * length.get());
        guard.set_cur_pos(real_pos);
        Ok(ClipChangedEvent::ClipPositionChanged(position))
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ClipPlayState {
    Stopped,
    Playing,
    Paused,
}

type TransitionResult = Result<State, (State, &'static str)>;

#[derive(Debug)]
enum State {
    Empty,
    Suspended(SuspendedState),
    Playing(PlayingState),
    Transitioning,
}

impl State {
    pub fn source(&self) -> Option<&OwnedSource> {
        use State::*;
        match self {
            Suspended(s) => Some(&s.source),
            Playing(s) => Some(&s.source),
            _ => None,
        }
    }

    pub fn play(
        self,
        reg: &SharedRegister,
        options: SlotPlayOptions,
        track: Option<&Track>,
        repeat: bool,
    ) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((Empty, "slot is empty")),
            Suspended(s) => s.play(reg, options, track, repeat),
            Playing(s) => s.play(reg, options, track, repeat),
            Transitioning => unreachable!(),
        }
    }

    pub fn stop(self, reg: &SharedRegister, immediately: bool) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Suspended(s) => s.stop(reg),
            Playing(s) => s.stop(reg, immediately),
            Transitioning => unreachable!(),
        }
    }

    pub fn pause(self) -> TransitionResult {
        use State::*;
        match self {
            s @ Empty | s @ Suspended(_) => Ok(s),
            Playing(s) => s.pause(),
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

    pub fn fill_with_source(self, source: OwnedSource, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty | Suspended(_) => {
                let mut g = lock(reg);
                g.set_src(Some(source.raw()));
                g.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(Suspended(SuspendedState {
                    source,
                    is_paused: false,
                }))
            }
            Playing(s) => s.fill_with_source(source, reg),
            Transitioning => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct SuspendedState {
    source: OwnedSource,
    is_paused: bool,
}

impl SuspendedState {
    pub fn play(
        self,
        reg: &SharedRegister,
        options: SlotPlayOptions,
        track: Option<&Track>,
        repeat: bool,
    ) -> TransitionResult {
        {
            let mut guard = lock(reg);
            guard.set_preview_track(track.map(|t| t.raw()));
            // The looped field might have been reset on non-immediate stop. Set it again.
            guard.set_looped(repeat);
        }
        let buffering_behavior = if options.is_effectively_buffered() {
            BitFlags::from_flag(BufferingBehavior::BufferSource)
        } else {
            BitFlags::empty()
        };
        let measure_alignment = if options.next_bar {
            MeasureAlignment::AlignWithMeasureStart
        } else {
            MeasureAlignment::PlayImmediately
        };
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
        match result {
            Ok(handle) => {
                let next_state = PlayingState {
                    source: self.source,
                    handle,
                    track: track.cloned(),
                };
                Ok(State::Playing(next_state))
            }
            Err(_) => Err((State::Suspended(self), "couldn't play preview")),
        }
    }

    pub fn stop(self, reg: &SharedRegister) -> TransitionResult {
        let next_state = State::Suspended(self);
        let mut g = lock(reg);
        // Reset position!
        g.set_cur_pos(PositionInSeconds::new(0.0));
        Ok(next_state)
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        let mut g = lock(reg);
        g.set_src(None);
        g.set_cur_pos(PositionInSeconds::new(0.0));
        Ok(State::Empty)
    }
}

#[derive(Debug)]
struct PlayingState {
    source: OwnedSource,
    handle: NonNull<raw::preview_register_t>,
    track: Option<Track>,
}

impl PlayingState {
    pub fn play(
        self,
        reg: &SharedRegister,
        options: SlotPlayOptions,
        track: Option<&Track>,
        repeat: bool,
    ) -> TransitionResult {
        if self.track.as_ref() != track {
            // Track change!
            self.suspend(true).play(reg, options, track, repeat)
        } else {
            let mut g = lock(reg);
            // Retrigger!
            g.set_cur_pos(PositionInSeconds::new(0.0));
            Ok(State::Playing(self))
        }
    }

    pub fn fill_with_source(self, source: OwnedSource, reg: &SharedRegister) -> TransitionResult {
        let mut g = lock(reg);
        g.set_src(Some(source.raw()));
        Ok(State::Playing(PlayingState {
            source,
            handle: self.handle,
            track: self.track,
        }))
    }

    pub fn stop(self, reg: &SharedRegister, immediately: bool) -> TransitionResult {
        if immediately {
            let suspended = self.suspend(false);
            let mut g = lock(reg);
            // Reset position!
            g.set_cur_pos(PositionInSeconds::new(0.0));
            Ok(State::Suspended(suspended))
        } else {
            lock(reg).set_looped(false);
            Ok(State::Playing(self))
        }
    }

    pub fn clear(self, reg: &SharedRegister) -> TransitionResult {
        self.suspend(false).clear(reg)
    }

    pub fn pause(self) -> TransitionResult {
        Ok(State::Suspended(self.suspend(true)))
    }

    fn suspend(self, pause: bool) -> SuspendedState {
        let next_state = SuspendedState {
            source: self.source,
            is_paused: pause,
        };
        // If not successful this probably means it was stopped already, so okay.
        if let Some(track) = self.track {
            let _ = unsafe {
                Reaper::get()
                    .medium_session()
                    .stop_track_preview_2(track.project().context(), self.handle)
            };
        } else {
            let _ = unsafe { Reaper::get().medium_session().stop_preview(self.handle) };
        };
        next_state
    }
}

pub struct ClipInfo {
    pub r#type: String,
    pub file_name: Option<PathBuf>,
    pub length: Option<DurationInSeconds>,
}

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {
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
    position: PositionInSeconds,
    length: Option<DurationInSeconds>,
) -> UnitValue {
    if let Some(l) = length {
        if l.get() == 0.0 {
            UnitValue::MIN
        } else {
            UnitValue::new_clamped(position.get() / l.get())
        }
    } else {
        UnitValue::MIN
    }
}
