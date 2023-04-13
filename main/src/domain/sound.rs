use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{
    FlexibleOwnedPcmSource, MeasureAlignment, MidiImportBehavior, OwnedPreviewRegister,
    PositionInSeconds, ReaperMutex, ReaperMutexGuard, ReaperVolumeValue,
};
use std::cell::Cell;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SoundPlayer {
    preview_register: Arc<ReaperMutex<OwnedPreviewRegister>>,
    play_handle: Cell<Option<NonNull<raw::preview_register_t>>>,
}

unsafe impl Send for SoundPlayer {}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundPlayer {
    pub fn new() -> Self {
        let mut register = OwnedPreviewRegister::new();
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        let preview_register = Arc::new(ReaperMutex::new(register));
        Self {
            preview_register,
            play_handle: Cell::new(None),
        }
    }

    pub fn load_file(&mut self, path_to_file: &Path) -> Result<(), &'static str> {
        if !path_to_file.exists() {
            return Err("sound file doesn't exist");
        }
        let source = Reaper::get()
            .medium_reaper()
            .pcm_source_create_from_file_ex(path_to_file, MidiImportBehavior::UsePreference)
            .map_err(|e| e.message())?;
        let mut preview_register = self.lock_preview_register()?;
        preview_register.set_src(Some(FlexibleOwnedPcmSource::Reaper(source)));
        Ok(())
    }

    pub fn play(&self) -> Result<(), &'static str> {
        if self.play_handle.get().is_some() {
            // Is playing already. Simply rewind.
            let mut preview_register = self.lock_preview_register()?;
            preview_register.set_cur_pos(PositionInSeconds::ZERO);
        } else {
            // Is not yet playing. Start playing.
            let handle = Reaper::get()
                .medium_session()
                .play_preview_ex(
                    self.preview_register.clone(),
                    Default::default(),
                    MeasureAlignment::PlayImmediately,
                )
                .map_err(|e| e.message())?;
            self.play_handle.set(Some(handle));
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<(), &'static str> {
        let play_handle = self.play_handle.take().ok_or("not playing")?;
        Reaper::get()
            .medium_session()
            .stop_preview(play_handle)
            .map_err(|e| e.message())?;
        self.lock_preview_register()?
            .set_cur_pos(PositionInSeconds::ZERO);
        Ok(())
    }

    fn lock_preview_register(
        &self,
    ) -> Result<ReaperMutexGuard<OwnedPreviewRegister>, &'static str> {
        self.preview_register
            .lock()
            .map_err(|_| "couldn't acquire preview register lock in sound player")
    }
}
