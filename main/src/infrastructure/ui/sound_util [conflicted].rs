use crate::infrastructure::plugin::App;
use reaper_high::Reaper;
use reaper_medium::{FlexibleOwnedPcmSource, MidiImportBehavior, OwnedPreviewRegister, ReaperVolumeValue};

pub fn play_sound(file_name: &str) -> Result<(), &'static str>{
    let mut register = OwnedPreviewRegister::new();
    let success_sound_path = App::realearn_sound_dir_path().join(file_name);
    let source = Reaper::get()
        .medium_reaper()
        .pcm_source_create_from_file_ex(&success_sound_path, MidiImportBehavior::UsePreference)
        .map_err(|e| e.message())?;
    register.set_src(Some(FlexibleOwnedPcmSource::Reaper(source)));
    register.set_volume()
    // Reaper::get().medium_session().play_preview_ex()
    // TODO-high Implement CONTINUE
}
