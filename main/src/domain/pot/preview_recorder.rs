use crate::base::hash_util::PersistentHash;
use crate::base::{blocking_lock_arc, file_util, Global};
use crate::domain::pot::{
    pot_db, Destination, LoadPresetOptions, LoadPresetWindowBehavior, PresetId,
    SharedRuntimePotUnit,
};
use reaper_high::{Project, Reaper};
use reaper_medium::{CommandId, OpenProjectBehavior, ProjectContext, ProjectInfoAttributeKey};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn record_previews(
    shared_pot_unit: SharedRuntimePotUnit,
    preset_ids: Vec<PresetId>,
    preview_rpp: PathBuf,
) {
    Global::future_support().spawn_in_main_thread_from_main_thread(async move {
        record_previews_async(shared_pot_unit, preset_ids, &preview_rpp).await?;
        Ok(())
    });
}

async fn record_previews_async(
    shared_pot_unit: SharedRuntimePotUnit,
    preset_ids: Vec<PresetId>,
    preview_rpp: &Path,
) -> Result<(), Box<dyn Error>> {
    let reaper = Reaper::get();
    let reaper_resource_dir = reaper.resource_path();
    // Open preview project template in new tab
    let project = open_preview_project_in_new_tab(preview_rpp);
    moment().await;
    // Prepare destination (first track, first FX)
    let first_track = project
        .first_track()
        .ok_or("preview must have at least one track")?;
    let destination = Destination {
        chain: first_track.normal_fx_chain(),
        fx_index: 0,
    };
    // Loop over the preset list
    for preset_id in preset_ids {
        let preset = pot_db()
            .find_preset_by_id(preset_id)
            .ok_or("preset not found")?;
        // Prefer creating preview file name based on preset content. That means whenever the
        // content changes, we get a different preview file name. That is cool.
        let hash = preset.common.content_or_id_hash();
        let preview_file_path = get_preview_file_path_from_hash(&reaper_resource_dir, hash);
        let options = LoadPresetOptions {
            window_behavior: LoadPresetWindowBehavior::AlwaysShow,
        };
        blocking_lock_arc(&shared_pot_unit, "record_previews pot unit").load_preset_at(
            &preset,
            options,
            &|_| Ok(destination.clone()),
        )?;
        moment().await;
        render_to_file(project, &preview_file_path)?;
        moment().await;
    }
    Ok(())
}

fn render_to_file(project: Project, full_path: &Path) -> Result<(), Box<dyn Error>> {
    let reaper = Reaper::get();
    let medium_reaper = reaper.medium_reaper();
    let dir = full_path.parent().ok_or("render path has not parent")?;
    let dir = dir.to_str().ok_or("render dir not valid UTF-8")?;
    let file_name = full_path
        .file_name()
        .ok_or("render path has no file name")?;
    let file_name = file_name
        .to_str()
        .ok_or("render file name not valid UTF-8")?;
    medium_reaper.get_set_project_info_string_set(
        ProjectContext::Proj(project.raw()),
        ProjectInfoAttributeKey::RenderFile,
        dir,
    )?;
    medium_reaper.get_set_project_info_string_set(
        ProjectContext::Proj(project.raw()),
        ProjectInfoAttributeKey::RenderPattern,
        file_name,
    )?;
    // "File: Render project, using the most recent render settings, auto-close render dialog"
    reaper
        .main_section()
        .action_by_command_id(CommandId::new(42230))
        .invoke_as_trigger(Some(project))?;
    Ok(())
}

fn open_preview_project_in_new_tab(preview_rpp: &Path) -> Project {
    let reaper = Reaper::get();
    let project = reaper.create_empty_project_in_new_tab();
    let mut behavior = OpenProjectBehavior::default();
    behavior.prompt = false;
    behavior.open_as_template = true;
    reaper
        .medium_reaper()
        .main_open_project(preview_rpp, behavior);
    project
}

async fn moment() {
    millis(200).await;
}

async fn millis(amount: u64) {
    futures_timer::Delay::new(Duration::from_millis(amount)).await;
}

pub fn get_preview_file_path_from_hash(
    reaper_resource_dir: &Path,
    hash: PersistentHash,
) -> PathBuf {
    let file_name = file_util::convert_hash_to_dir_structure(hash, ".ogg");
    reaper_resource_dir
        .join("Helgoboss/Pot/previews")
        .join(&file_name)
}
