use reaper_high::{Reaper, Track};
use reaper_low::raw;
use reaper_medium::{
    MidiImportBehavior, OwnedPreviewRegister, PlayingPreviewRegister, ReaperLockError,
    ReaperVolumeValue,
};
use std::mem;
use std::path::Path;
use std::ptr::{null_mut, NonNull};

/// Manages PCM source lifetimes.
#[derive(Default)]
pub struct PreviewSlot {
    state: State,
}

enum State {
    Stopped(StoppedState),
    PlayRequested,
    Playing(PlayingState),
}

struct StoppedState {
    register: OwnedPreviewRegister,
}

struct PlayingState {
    register: PlayingPreviewRegister,
}

impl Default for StoppedState {
    fn default() -> Self {
        let mut register = OwnedPreviewRegister::default();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        Self { register }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::Stopped(Default::default())
    }
}

impl PreviewSlot {
    pub fn fill_with_file(&mut self, file: &Path) -> Result<(), &'static str> {
        let pcm_source = Reaper::get()
            .medium_reaper()
            .pcm_source_create_from_file_ex(file, MidiImportBehavior::UsePreference)
            .map_err(|_| "couldn't create PCM source")?;
        // Destroy old source if any
        // TODO-high What if the preview is still playing?
        match &mut self.state {
            State::Stopped(s) => {
                if let Some(source) = s.register.src() {
                    unsafe {
                        Reaper::get().medium_reaper().pcm_source_destroy(source);
                    }
                }
                s.register.set_src(Some(pcm_source));
                Ok(())
            }
            State::PlayRequested => Err("starting to play"),
            State::Playing(_) => Err("still playing"),
        }
    }

    pub fn is_filled(&self) -> bool {
        match &self.state {
            State::Stopped(s) => s.register.src().is_some(),
            State::PlayRequested => true,
            State::Playing(_) => true,
        }
    }

    pub fn play(&mut self, track: Option<&Track>) -> Result<(), &'static str> {
        let next_state = match mem::replace(&mut self.state, State::PlayRequested) {
            State::Stopped(state) => {
                let register = unsafe {
                    Reaper::get()
                        .medium_session()
                        .play_preview_ex(state.register, 0, 0.0)
                        .map_err(|_| "couldn't play preview")?
                };
                State::Playing(PlayingState { register })
            }
            State::PlayRequested => return Err("play has already been requested"),
            State::Playing(state) => {
                state.register.lock(|result| -> Result<(), &'static str> {
                    let register = result.map_err(|_| "couldn't acquire lock")?;
                    register.set_cur_pos(0.0);
                    Ok(())
                })?;
                State::Playing(PlayingState {
                    register: state.register,
                })
            }
        };
        self.state = next_state;
        Ok(())
    }
}
