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

#[derive(Debug)]
enum State {
    Empty,
    Stopped(StoppedState),
    Playing(PlayingState),
    Transition,
}

#[derive(Debug)]
struct StoppedState {
    source: PcmSource,
}

impl StoppedState {
    fn play(self, register: &SharedRegister) -> Result<PlayingState, (&'static str, StoppedState)> {
        let result = unsafe {
            Reaper::get().medium_session().play_preview_ex(
                register.clone(),
                BitFlags::empty(),
                MeasureAlignment::PlayImmediately,
            )
        };
        match result {
            Ok(handle) => {
                let next_state = PlayingState {
                    source: self.source,
                    handle,
                };
                Ok(next_state)
            }
            Err(_) => Err(("couldn't play preview", self)),
        }
    }
}

#[derive(Debug)]
struct PlayingState {
    source: PcmSource,
    handle: NonNull<raw::preview_register_t>,
}

impl PlayingState {
    fn play(self, register: &SharedRegister) -> Result<PlayingState, (&'static str, PlayingState)> {
        match register.lock() {
            Ok(mut guard) => {
                guard.set_cur_pos(PositionInSeconds::new(0.0));
                Ok(self)
            }
            Err(_) => Err(("couldn't acquire lock", self)),
        }
    }

    fn stop(self) -> StoppedState {
        let _ = unsafe { Reaper::get().medium_session().stop_preview(self.handle) };
        StoppedState {
            source: self.source,
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
        let (next_state, result) = match mem::replace(&mut self.state, State::Transition) {
            State::Empty | State::Stopped(_) => match self.set_register_source(&source) {
                Ok(_) => (State::Stopped(StoppedState { source }), Ok(())),
                Err(e) => (State::Empty, Err(e)),
            },
            State::Playing(old_playing) => {
                // Important to stop before we destroy the existing source.
                let mut stopped = old_playing.stop();
                match self.set_register_source(&source) {
                    Ok(_) => {
                        stopped.source = source;
                        match stopped.play(&self.register) {
                            Ok(new_playing) => (State::Playing(new_playing), Ok(())),
                            Err((e, stopped)) => (State::Stopped(stopped), Err(e)),
                        }
                    }
                    Err(e) => (State::Stopped(stopped), Err(e)),
                }
            }
            State::Transition => unreachable!(),
        };
        self.state = next_state;
        result
    }

    pub fn play(&mut self, track: Option<&Track>) -> Result<(), &'static str> {
        let (next_state, result) = match mem::replace(&mut self.state, State::Transition) {
            State::Empty => (State::Empty, Err("slot is empty")),
            State::Stopped(stopped) => match stopped.play(&self.register) {
                Ok(playing) => (State::Playing(playing), Ok(())),
                Err((e, stopped)) => (State::Stopped(stopped), Err(e)),
            },
            State::Playing(old_playing) => match old_playing.play(&self.register) {
                Ok(new_playing) => (State::Playing(new_playing), Ok(())),
                Err((e, old_playing)) => (State::Playing(old_playing), Err(e)),
            },
            State::Transition => unreachable!(),
        };
        self.state = next_state;
        result
    }

    pub fn stop(&mut self) {
        self.state = match mem::replace(&mut self.state, State::Transition) {
            s @ State::Empty | s @ State::Stopped(_) => s,
            State::Playing(playing) => State::Stopped(playing.stop()),
            State::Transition => unreachable!(),
        };
    }

    fn set_register_source(&self, source: &PcmSource) -> Result<(), &'static str> {
        self.register
            .lock()
            .map_err(|_| "couldn't acquire lock")?
            .set_src(Some(source.raw()));
        Ok(())
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
