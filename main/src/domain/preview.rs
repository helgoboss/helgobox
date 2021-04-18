use enumflags2::BitFlags;
use reaper_high::{Item, Reaper, Track};
use reaper_low::raw;
use reaper_low::raw::preview_register_t;
use reaper_medium::{
    BufferingBehavior, MeasureAlignment, MediaItem, MidiImportBehavior, OwnedPreviewRegister,
    PositionInSeconds, ReaperFunctionError, ReaperLockError, ReaperMutex, ReaperMutexGuard,
    ReaperVolumeValue,
};
use std::mem;
use std::path::Path;
use std::ptr::{null_mut, NonNull};
use std::sync::Arc;

type SharedRegister = Arc<ReaperMutex<OwnedPreviewRegister>>;

/// Manages PCM source lifetimes.
#[derive(Debug)]
pub struct PreviewSlot {
    register: SharedRegister,
    state: State,
}

impl Default for PreviewSlot {
    fn default() -> Self {
        let mut register = OwnedPreviewRegister::default();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        Self {
            register: Arc::new(ReaperMutex::new(register)),
            state: State::Empty,
        }
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
    pub fn play(self, reg: &SharedRegister, options: SlotPlayOptions) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((Empty, "slot is empty")),
            Suspended(s) => s.play(reg, options),
            Playing(s) => s.play(reg),
            Transitioning => unreachable!(),
        }
    }

    pub fn stop(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Ok(Empty),
            Suspended(s) => s.stop(reg),
            Playing(s) => s.stop(reg),
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

    pub fn fill_with_source(self, source: PcmSource, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty | Suspended(_) => match lock(reg) {
                Ok(mut g) => {
                    g.set_src(Some(source.raw()));
                    g.set_cur_pos(PositionInSeconds::new(0.0));
                    Ok(Suspended(SuspendedState {
                        source,
                        is_paused: false,
                    }))
                }
                Err(e) => Err((Empty, e)),
            },
            Playing(s) => s.fill_with_source(source, reg),
            Transitioning => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct SuspendedState {
    source: PcmSource,
    is_paused: bool,
}

impl SuspendedState {
    pub fn play(self, register: &SharedRegister, options: SlotPlayOptions) -> TransitionResult {
        let result = unsafe {
            Reaper::get().medium_session().play_preview_ex(
                register.clone(),
                if options.buffered {
                    BitFlags::from_flag(BufferingBehavior::BufferSource)
                } else {
                    BitFlags::empty()
                },
                if options.next_bar {
                    MeasureAlignment::AlignWithMeasureStart
                } else {
                    MeasureAlignment::PlayImmediately
                },
            )
        };
        match result {
            Ok(handle) => {
                let next_state = PlayingState {
                    source: self.source,
                    handle,
                };
                Ok(State::Playing(next_state))
            }
            Err(_) => Err((State::Suspended(self), "couldn't play preview")),
        }
    }

    pub fn stop(self, reg: &SharedRegister) -> TransitionResult {
        let next_state = State::Suspended(self);
        match lock(reg) {
            Ok(mut guard) => {
                // Reset position!
                guard.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(next_state)
            }
            Err(e) => Err((next_state, e)),
        }
    }
}

#[derive(Debug)]
struct PlayingState {
    source: PcmSource,
    handle: NonNull<raw::preview_register_t>,
}

impl PlayingState {
    pub fn play(self, reg: &SharedRegister) -> TransitionResult {
        match lock(reg) {
            Ok(mut guard) => {
                // Retrigger!
                guard.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(State::Playing(self))
            }
            Err(e) => Err((State::Playing(self), e)),
        }
    }

    pub fn fill_with_source(self, source: PcmSource, reg: &SharedRegister) -> TransitionResult {
        match lock(reg) {
            Ok(mut g) => {
                g.set_src(Some(source.raw()));
                Ok(State::Playing(PlayingState {
                    source,
                    handle: self.handle,
                }))
            }
            Err(e) => Err((State::Playing(self), e)),
        }
    }

    pub fn stop(self, reg: &SharedRegister) -> TransitionResult {
        let next_state = self.suspend(false);
        match lock(reg) {
            Ok(mut guard) => {
                // Reset position!
                guard.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(next_state)
            }
            Err(e) => Err((next_state, e)),
        }
    }

    pub fn pause(self) -> TransitionResult {
        Ok(self.suspend(true))
    }

    fn suspend(self, pause: bool) -> State {
        let next_state = State::Suspended(SuspendedState {
            source: self.source,
            is_paused: pause,
        });
        // If not successful this probably means it was stopped already, so okay.
        let _ = unsafe { Reaper::get().medium_session().stop_preview(self.handle) };
        next_state
    }
}

impl PreviewSlot {
    pub fn fill_with_source_from_item(&mut self, item: Item) -> Result<(), &'static str> {
        let source = item
            .active_take()
            .ok_or("item has no active take")?
            .source()
            .ok_or("take has no source")?;
        let owned_source = PcmSource::new(source.raw());
        self.fill_with_source(owned_source)
    }

    pub fn fill_with_source_from_file(&mut self, file: &Path) -> Result<(), &'static str> {
        let raw_source = unsafe {
            Reaper::get()
                .medium_reaper()
                .pcm_source_create_from_file_ex(file, MidiImportBehavior::UsePreference)
                .map_err(|_| "couldn't create PCM source")?
        };
        let owned_source = PcmSource::new(raw_source);
        self.fill_with_source(owned_source)
    }

    pub fn is_filled(&self) -> bool {
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

    pub fn fill_with_source(&mut self, source: PcmSource) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .fill_with_source(source, &self.register);
        self.finish_transition(result)
    }

    pub fn play(
        &mut self,
        track: Option<&Track>,
        options: SlotPlayOptions,
    ) -> Result<(), &'static str> {
        let result = self.start_transition().play(&self.register, options);
        self.finish_transition(result)
    }

    pub fn stop(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().stop(&self.register);
        self.finish_transition(result)
    }

    pub fn pause(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().pause();
        self.finish_transition(result)
    }

    pub fn is_looped(&self) -> Result<bool, &'static str> {
        Ok(lock(&self.register)?.is_looped())
    }

    pub fn toggle_looped(&mut self) -> Result<bool, &'static str> {
        let mut guard = lock(&self.register)?;
        let new_value = !guard.is_looped();
        guard.set_looped(new_value);
        Ok(new_value)
    }

    pub fn volume(&self) -> Result<ReaperVolumeValue, &'static str> {
        Ok(lock(&self.register)?.volume())
    }

    pub fn set_volume(&mut self, volume: ReaperVolumeValue) -> Result<(), &'static str> {
        lock(&self.register)?.set_volume(volume);
        Ok(())
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

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct SlotPlayOptions {
    pub next_bar: bool,
    pub buffered: bool,
}

/// Owned PCM source.
#[derive(Debug)]
pub struct PcmSource {
    raw: NonNull<raw::PCM_source>,
}

impl PcmSource {
    pub fn new(raw: NonNull<raw::PCM_source>) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> NonNull<raw::PCM_source> {
        self.raw
    }
}

impl Drop for PcmSource {
    fn drop(&mut self) {
        // TODO-high Attention! To make this work, we need to duplicate an item source to make it
        // owned!!!
        unsafe {
            // Reaper::get().medium_reaper().pcm_source_destroy(self.raw);
        }
    }
}

fn lock(reg: &SharedRegister) -> Result<ReaperMutexGuard<OwnedPreviewRegister>, &'static str> {
    reg.lock().map_err(|_| "couldn't acquire lock")
}
