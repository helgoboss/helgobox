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

type TransitionResult = Result<State, (State, &'static str)>;

#[derive(Debug)]
enum State {
    Empty,
    Stopped(StoppedState),
    Playing(PlayingState),
    Transitioning,
}

impl State {
    pub fn play(self, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty => Err((State::Empty, "slot is empty")),
            Stopped(s) => s.play(reg),
            Playing(s) => s.play(reg),
            Transitioning => unreachable!(),
        }
    }

    pub fn stop(self) -> TransitionResult {
        use State::*;
        match self {
            s @ Empty | s @ Stopped(_) => Ok(s),
            Playing(s) => s.stop(),
            Transitioning => unreachable!(),
        }
    }

    pub fn fill_with_source(self, source: PcmSource, reg: &SharedRegister) -> TransitionResult {
        use State::*;
        match self {
            Empty | Stopped(_) => match set_register_source(reg, &source) {
                Ok(_) => Ok(Stopped(StoppedState { source })),
                Err(e) => Err((Empty, e)),
            },
            Playing(s) => {
                // Important to stop before we destroy the existing source.
                let stopped = s.stop()?;
                stopped.play(reg)
            }
            Transitioning => unreachable!(),
        }
    }
}

#[derive(Debug)]
struct StoppedState {
    source: PcmSource,
}

impl StoppedState {
    fn play(self, register: &SharedRegister) -> TransitionResult {
        let result = unsafe {
            Reaper::get().medium_session().play_preview_ex(
                register.clone(),
                BufferingBehavior::BufferSource | BufferingBehavior::VariSpeed,
                MeasureAlignment::AlignWithMeasureStart,
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
            Err(_) => Err((State::Stopped(self), "couldn't play preview")),
        }
    }
}

#[derive(Debug)]
struct PlayingState {
    source: PcmSource,
    handle: NonNull<raw::preview_register_t>,
}

impl PlayingState {
    fn play(self, register: &SharedRegister) -> TransitionResult {
        match register.lock() {
            Ok(mut guard) => {
                guard.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(State::Playing(self))
            }
            Err(_) => Err((State::Playing(self), "couldn't acquire lock")),
        }
    }

    fn stop(self) -> TransitionResult {
        let next_state = State::Stopped(StoppedState {
            source: self.source,
        });
        match unsafe { Reaper::get().medium_session().stop_preview(self.handle) } {
            Ok(_) => Ok(next_state),
            Err(_) => Err((next_state, "couldn't stop, hopefully stopped already")),
        }
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

    pub fn fill_with_source(&mut self, source: PcmSource) -> Result<(), &'static str> {
        let result = self
            .start_transition()
            .fill_with_source(source, &self.register);
        self.finish_transition(result)
    }

    pub fn play(&mut self, track: Option<&Track>) -> Result<(), &'static str> {
        let result = self.start_transition().play(&self.register);
        self.finish_transition(result)
    }

    pub fn stop(&mut self) -> Result<(), &'static str> {
        let result = self.start_transition().stop();
        self.finish_transition(result)
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

fn set_register_source(reg: &SharedRegister, source: &PcmSource) -> Result<(), &'static str> {
    reg.lock()
        .map_err(|_| "couldn't acquire lock")?
        .set_src(Some(source.raw()));
    Ok(())
}
