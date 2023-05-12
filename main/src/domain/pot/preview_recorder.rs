use crate::base::future_util::millis;
use crate::base::hash_util::PersistentHash;
use crate::base::{blocking_lock_arc, file_util, Global};
use crate::domain::pot::provider_database::{
    FIL_IS_AVAILABLE_TRUE, FIL_IS_SUPPORTED_TRUE, FIL_PRODUCT_KIND_INSTRUMENT,
};
use crate::domain::pot::{
    pot_db, preview_exists, BuildInput, Destination, FilterItemId, LoadPresetOptions,
    LoadPresetWindowBehavior, PluginId, PresetId, PresetKind, PresetWithId, ProductId,
    SharedRuntimePotUnit,
};
use realearn_api::persistence::PotFilterKind;
use reaper_high::{Project, Reaper};
use reaper_medium::{CommandId, OpenProjectBehavior, ProjectContext, ProjectInfoAttributeKey};
use std::error::Error;
use std::path::{Path, PathBuf};

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

pub fn get_preview_file_path_from_hash(
    reaper_resource_dir: &Path,
    hash: PersistentHash,
) -> PathBuf {
    let file_name = file_util::convert_hash_to_dir_structure(hash, ".ogg");
    reaper_resource_dir
        .join("Helgoboss/Pot/previews")
        .join(&file_name)
}

/// Can take long.
pub fn prepare_preview_recording(mut build_input: BuildInput) -> Vec<PresetWithId> {
    // We want only available and supported instruments
    build_input.filters.set(
        PotFilterKind::ProductKind,
        Some(FilterItemId(Some(FIL_PRODUCT_KIND_INSTRUMENT))),
    );
    build_input.filters.set(
        PotFilterKind::IsAvailable,
        Some(FilterItemId(Some(FIL_IS_AVAILABLE_TRUE))),
    );
    build_input.filters.set(
        PotFilterKind::IsSupported,
        Some(FilterItemId(Some(FIL_IS_SUPPORTED_TRUE))),
    );
    // Gather
    let mut presets = pot_db().gather_presets(build_input);
    // Take only those that don't have a preview yet
    let reaper_resource_dir = Reaper::get().resource_path();
    presets.retain(|p| !preview_exists(&p.preset, &reaper_resource_dir));
    // Sort by plug-in
    presets.sort_by(|left, right| bucket(left).cmp(&bucket(right)));
    presets
}

fn bucket(preset_with_id: &PresetWithId) -> BucketId {
    let preset = &preset_with_id.preset;
    if !preset.common.plugin_ids.is_empty() {
        return BucketId::Plugin(&preset.common.plugin_ids);
    }
    if !preset.common.product_ids.is_empty() {
        return BucketId::ProductId(&preset.common.product_ids);
    }
    if let PresetKind::FileBased(kind) = &preset.kind {
        return BucketId::FileExtension(&kind.file_ext);
    }
    BucketId::Remaining
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum BucketId<'a> {
    /// The plug-in ID is certainly the best criteria here! We have that for all databases except
    /// Komplete. For FX chains and track templates, we might have multiple plug-ins. The order of
    /// these plug-ins is important because different orders will reload the plug-ins.
    Plugin(&'a [PluginId]),
    /// The next best bet is
    ProductId(&'a [ProductId]),
    /// And finally: File extension.
    FileExtension(&'a str),
    Remaining,
}
