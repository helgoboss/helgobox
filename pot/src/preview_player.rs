use anyhow::Context;
use base::{SoundPlayer, blocking_lock};
use once_cell::sync::Lazy;
use reaper_high::Reaper;
use reaper_medium::ReaperVolumeValue;
use std::sync::Mutex;

use crate::{PresetId, find_preview_file, pot_db};

static PREVIEW_PLAYER: Lazy<Mutex<SoundPlayer>> = Lazy::new(|| Mutex::new(SoundPlayer::new()));

pub fn play_preview(preset_id: PresetId) -> anyhow::Result<()> {
    let preset = pot_db()
        .find_preset_by_id(preset_id)
        .context("couldn't find preset")?;

    let resource_dir = Reaper::get().resource_path();
    let preview_file =
        find_preview_file(&preset, &resource_dir).context("couldn't find preview file")?;

    let mut player = blocking_lock(&PREVIEW_PLAYER, "play pot preview");
    player.load_file(&preview_file)?;
    player.play()
}

pub fn stop_preview() -> anyhow::Result<()> {
    blocking_lock(&PREVIEW_PLAYER, "stop pot preview").stop()
}

pub fn set_preview_volume(volume: ReaperVolumeValue) -> anyhow::Result<()> {
    blocking_lock(&PREVIEW_PLAYER, "set pot preview volume").set_volume(volume)
}

pub fn get_preview_volume() -> ReaperVolumeValue {
    blocking_lock(&PREVIEW_PLAYER, "set pot preview volume")
        .volume()
        .unwrap_or_default()
}
