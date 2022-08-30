use crate::infrastructure::plugin::App;
use reaper_high::Reaper;
use reaper_medium::{
    FlexibleOwnedPcmSource, MeasureAlignment, MidiImportBehavior, OwnedPreviewRegister,
    PositionInSeconds, ReaperMutex, ReaperVolumeValue,
};
use std::cell::Cell;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Sound {
    preview_register: Arc<ReaperMutex<OwnedPreviewRegister>>,
    is_playing_already: Cell<bool>,
}

impl Sound {
    pub fn from_file(file_name: &str) -> Result<Sound, &'static str> {
        let mut register = OwnedPreviewRegister::new();
        let path_to_file = App::realearn_sound_dir_path().join(file_name);
        if !path_to_file.exists() {
            return Err("couldn't find sound file");
        }
        let source = Reaper::get()
            .medium_reaper()
            .pcm_source_create_from_file_ex(&path_to_file, MidiImportBehavior::UsePreference)
            .map_err(|e| e.message())?;
        register.set_src(Some(FlexibleOwnedPcmSource::Reaper(source)));
        register.set_volume(ReaperVolumeValue::ZERO_DB);
        let preview_register = Arc::new(ReaperMutex::new(register));
        let sound = Sound {
            preview_register,
            is_playing_already: Cell::new(false),
        };
        Ok(sound)
    }

    pub fn play(&self) -> Result<(), &'static str> {
        if self.is_playing_already.get() {
            let mut preview_register = self
                .preview_register
                .lock()
                .map_err(|_| "couldn't acquire lock for playing sound")?;
            preview_register.set_cur_pos(PositionInSeconds::ZERO);
        } else {
            Reaper::get()
                .medium_session()
                .play_preview_ex(
                    self.preview_register.clone(),
                    Default::default(),
                    MeasureAlignment::PlayImmediately,
                )
                .map_err(|e| e.message())?;
            self.is_playing_already.set(true);
        }
        Ok(())
    }
}
