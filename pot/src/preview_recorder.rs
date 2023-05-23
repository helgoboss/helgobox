use crate::provider_database::{
    FIL_IS_AVAILABLE_TRUE, FIL_IS_SUPPORTED_TRUE, FIL_PRODUCT_KIND_INSTRUMENT,
};
use crate::{
    pot_db, preview_exists, BuildInput, Destination, EscapeCatcher, FilterItemId,
    LoadPresetOptions, LoadPresetWindowBehavior, PluginId, Preset, PresetKind, PresetWithId,
    ProductId, SharedRuntimePotUnit,
};
use base::future_util::millis;
use base::hash_util::PersistentHash;
use base::{blocking_lock_arc, blocking_write_lock, file_util};
use realearn_api::persistence::PotFilterKind;
use reaper_high::{Project, Reaper};
use reaper_medium::{CommandId, OpenProjectBehavior, ProjectContext, ProjectInfoAttributeKey};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub type SharedPreviewRecorderState = Arc<RwLock<PreviewRecorderState>>;

#[derive(Debug)]
pub struct PreviewRecorderState {
    pub todos: Vec<PresetWithId>,
    pub failures: Vec<PreviewRecorderFailure>,
}

impl PreviewRecorderState {
    pub fn new(todos: Vec<PresetWithId>) -> Self {
        Self {
            todos,
            failures: vec![],
        }
    }
}

#[derive(Debug)]
pub struct PreviewRecorderFailure {
    pub preset: PresetWithId,
    pub reason: String,
}

impl AsRef<Preset> for PreviewRecorderFailure {
    fn as_ref(&self) -> &Preset {
        &self.preset.preset
    }
}

pub async fn record_previews(
    shared_pot_unit: SharedRuntimePotUnit,
    state: SharedPreviewRecorderState,
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
    let cloned_state = state.clone();
    let report_failure = move |preset: PresetWithId, reason: String| {
        let mut state = blocking_write_lock(&cloned_state, "record_previews state 2");
        let failure = PreviewRecorderFailure { preset, reason };
        state.failures.push(failure);
    };
    let escape_catcher = EscapeCatcher::new();
    // Loop over the preset list
    loop {
        // Check if escape has been pressed
        if escape_catcher.escape_was_pressed() {
            break;
        }
        // Take new preset to be recorded
        moment().await;
        let Some(preset_with_id) = blocking_write_lock(&state, "record_previews state").todos.pop() else {
            // Done!
            break;
        };
        // Determine destination file
        let preset = &preset_with_id.preset;
        // Prefer creating preview file name based on preset content. That means whenever the
        // content changes, we get a different preview file name. That is cool.
        let hash = preset.common.content_or_id_hash();
        let preview_file_path = get_preview_file_path_from_hash(&reaper_resource_dir, hash);
        // Load preset
        let options = LoadPresetOptions {
            window_behavior: LoadPresetWindowBehavior::AlwaysShow,
            ..Default::default()
        };
        {
            let load_result = blocking_lock_arc(&shared_pot_unit, "record_previews pot unit")
                .load_preset_at(preset, options, &|_| Ok(destination.clone()));
            if let Err(e) = load_result {
                report_failure(preset_with_id, e.to_string());
                continue;
            }
        }
        moment().await;
        // Record preview
        if let Err(e) = render_to_file(project, &preview_file_path) {
            report_failure(preset_with_id, e.to_string());
        }
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
        .join(file_name)
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
    // Reverse, so we can efficiently pop from the front later on
    presets.reverse();
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
