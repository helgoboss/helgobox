use crate::application::{CompartmentPresetManager, CompartmentPresetModel};

use crate::base::notification;
use crate::base::notification::warn_user_on_anyhow_error;
use crate::domain::{Compartment, IncludedDirLuaModuleFinder, LuaModuleContainer, SafeLua};
use crate::infrastructure::api::convert::to_data::convert_compartment;
use crate::infrastructure::data::CompartmentPresetData;
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::util::open_in_file_manager;
use anyhow::{anyhow, bail, Context};
use base::byte_pattern::BytePattern;
use base::file_util;
use include_dir::{include_dir, Dir};
use itertools::Itertools;
use mlua::LuaSerdeExt;
use realearn_api::persistence::{
    CommonPresetMetaData, ControllerPresetMetaData, MainPresetMetaData, VirtualControlSchemeId,
};
use reaper_high::Reaper;
use rxrust::prelude::*;
use serde::Deserialize;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;
use std::{fmt, fs};
use strum::EnumIs;
use walkdir::WalkDir;

pub type SharedControllerPresetManager = Rc<RefCell<FileBasedControllerPresetManager>>;
pub type FileBasedControllerPresetManager =
    FileBasedCompartmentPresetManager<ControllerPresetMetaData>;

pub type SharedMainPresetManager = Rc<RefCell<FileBasedMainPresetManager>>;
pub type FileBasedMainPresetManager = FileBasedCompartmentPresetManager<MainPresetMetaData>;

#[derive(Debug)]
pub struct FileBasedCompartmentPresetManager<M> {
    compartment: Compartment,
    preset_dir_path: PathBuf,
    preset_infos: Vec<PresetInfo<M>>,
    changed_subject: LocalSubject<'static, (), ()>,
    event_handler: Box<dyn CompartmentPresetManagerEventHandler<Source = Self>>,
}

impl FileBasedCompartmentPresetManager<ControllerPresetMetaData> {
    pub fn find_controller_preset_compatible_with_device(
        &self,
        midi_identity_reply: &[u8],
    ) -> Option<&PresetInfo<ControllerPresetMetaData>> {
        self.preset_infos.iter().find(|info| {
            let Some(midi_identity_pattern) =
                info.specific_meta_data.midi_identity_pattern.as_ref()
            else {
                return false;
            };
            let Ok(byte_pattern) = BytePattern::from_str(midi_identity_pattern) else {
                return false;
            };
            byte_pattern.matches(midi_identity_reply)
        })
    }
}

#[derive(Copy, Clone)]
pub struct MainPresetSelectionConditions {
    pub at_least_one_instance_has_playtime_clip_matrix: bool,
}

impl FileBasedCompartmentPresetManager<MainPresetMetaData> {
    pub fn find_most_suitable_main_preset_for_schemes(
        &self,
        virtual_control_schemes: &HashSet<VirtualControlSchemeId>,
        conditions: MainPresetSelectionConditions,
    ) -> Option<&PresetInfo<MainPresetMetaData>> {
        let mut candidates: Vec<_> = self
            .preset_infos
            .iter()
            .filter_map(|info| {
                let intersection_count = info
                    .specific_meta_data
                    .used_schemes
                    .intersection(virtual_control_schemes)
                    .count();
                if intersection_count == 0 {
                    return None;
                }
                Some((info, intersection_count))
            })
            .collect();
        candidates.sort_unstable_by(|(preset_a, rank_a), (preset_b, rank_b)| {
            // Prefer Playtime preset if at least one instance has a Playtime clip matrix
            if conditions.at_least_one_instance_has_playtime_clip_matrix {
                let ord = preset_a
                    .specific_meta_data
                    .requires_playtime()
                    .cmp(&preset_b.specific_meta_data.requires_playtime());
                if ord.is_ne() {
                    return ord;
                }
            }
            // Apart from that, prefer higher rank
            rank_a.cmp(rank_b)
        });
        Some(candidates.into_iter().next()?.0)
    }
}

pub trait CompartmentPresetManagerEventHandler: fmt::Debug {
    type Source;

    fn presets_changed(&self, source: &Self::Source);
}

pub trait CommonCompartmentPresetManager {
    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()>;
    fn common_preset_infos(&self) -> Box<dyn Iterator<Item = &CommonPresetInfo> + '_>;
    fn common_preset_info_by_id(&self, id: &str) -> Option<&CommonPresetInfo>;
    fn save_original_factory_preset_as_user_preset(
        &mut self,
        relative_file_path: &Path,
    ) -> anyhow::Result<String>;
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct PresetInfo<S> {
    pub common: CommonPresetInfo,
    pub specific_meta_data: S,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CommonPresetInfo {
    pub id: String,
    pub file_type: PresetFileType,
    pub origin: PresetOrigin,
    pub meta_data: CommonPresetMetaData,
}

#[derive(Clone, Eq, PartialEq, Debug, EnumIs)]
pub enum PresetOrigin {
    User {
        /// The ID should actually be equal to the path, but to not run into any
        /// breaking change in edge cases, we keep track of the actual path here.
        absolute_file_path: PathBuf,
    },
    Factory {
        compartment: Compartment,
        relative_file_path: PathBuf,
    },
}

impl fmt::Display for PresetOrigin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetOrigin::User {
                absolute_file_path: absolute_path,
            } => absolute_path.display().fmt(f),
            PresetOrigin::Factory {
                compartment,
                relative_file_path,
            } => {
                let compartment_id = match compartment {
                    Compartment::Controller => "controller",
                    Compartment::Main => "main",
                };
                write!(
                    f,
                    "factory:/{compartment_id}/{}",
                    relative_file_path.display()
                )
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PresetFileType {
    Json,
    Lua,
}

impl PresetFileType {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?;
        let file_type = if extension == "json" {
            PresetFileType::Json
        } else if extension == "lua" {
            PresetFileType::Lua
        } else {
            return None;
        };
        Some(file_type)
    }
}

#[derive(Deserialize)]
struct CombinedPresetMetaData<S> {
    #[serde(flatten)]
    common: CommonPresetMetaData,
    #[serde(flatten)]
    specific: S,
}

pub trait SpecificPresetMetaData: fmt::Debug + for<'a> Deserialize<'a> {}

impl SpecificPresetMetaData for ControllerPresetMetaData {}

impl SpecificPresetMetaData for MainPresetMetaData {}

impl<S: SpecificPresetMetaData> CombinedPresetMetaData<S> {
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn from_lua_code(lua_code: &str) -> anyhow::Result<Self> {
        parse_lua_frontmatter(lua_code)
    }
}

impl<S: SpecificPresetMetaData> FileBasedCompartmentPresetManager<S> {
    pub fn new(
        compartment: Compartment,
        preset_dir_path: PathBuf,
        event_handler: Box<dyn CompartmentPresetManagerEventHandler<Source = Self>>,
    ) -> FileBasedCompartmentPresetManager<S> {
        FileBasedCompartmentPresetManager {
            compartment,
            preset_dir_path,
            preset_infos: vec![],
            changed_subject: Default::default(),
            event_handler,
        }
    }

    pub fn load_presets_from_disk(&mut self) -> Result<(), String> {
        self.load_presets_from_disk_without_notification()?;
        self.notify_presets_changed();
        Ok(())
    }

    pub fn load_presets_from_disk_without_notification(&mut self) -> Result<(), String> {
        // Load factory preset infos
        let compartment = self.compartment;
        let factory_preset_dir = get_factory_preset_dir(compartment);
        let mut all_preset_infos = vec![];
        walk_included_dir(factory_preset_dir, &mut |file| {
            let relative_file_path = file.path();
            let file_type = PresetFileType::from_path(relative_file_path)
                .context("Factory preset has unsupported file type")?;
            let file_content = file
                .contents_utf8()
                .context("Factory preset not UTF-8 encoded")?;
            let origin = PresetOrigin::Factory {
                compartment,
                relative_file_path: relative_file_path.to_path_buf(),
            };
            let preset_info = load_preset_info(
                origin,
                relative_file_path,
                file_type,
                "factory/",
                file_content,
            )?;
            all_preset_infos.push(preset_info);
            Ok(())
        });
        // Load user preset infos
        let user_preset_infos = WalkDir::new(&self.preset_dir_path)
            .follow_links(true)
            .max_depth(4)
            .into_iter()
            .filter_entry(|e| !file_util::is_hidden(e))
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                let relative_file_path = entry
                    .path()
                    .strip_prefix(&self.preset_dir_path)
                    .unwrap()
                    .to_path_buf();
                if relative_file_path.iter().next().is_some_and(|component| {
                    component.to_string_lossy().eq_ignore_ascii_case("factory")
                }) {
                    // User presets must not get mixed up with factory presets. Most importantly,
                    // it's not allowed to override factory presets. That could create mess.
                    return None;
                }
                let file_type = PresetFileType::from_path(entry.path())?;
                match self.build_user_preset_info(entry.into_path(), &relative_file_path, file_type)
                {
                    Ok(p) => Some(p),
                    Err(e) => {
                        notification::warn(e.to_string());
                        None
                    }
                }
            });
        // Combine them
        all_preset_infos.extend(user_preset_infos);
        self.preset_infos = all_preset_infos;
        Ok(())
    }

    fn build_user_preset_info(
        &mut self,
        absolute_file_path: PathBuf,
        relative_file_path: &Path,
        file_type: PresetFileType,
    ) -> anyhow::Result<PresetInfo<S>> {
        let origin = PresetOrigin::User {
            absolute_file_path: absolute_file_path.clone(),
        };
        let file_content = fs::read_to_string(&absolute_file_path).map_err(|_| {
            anyhow!(
                "Couldn't read preset file \"{}\".",
                absolute_file_path.display()
            )
        })?;
        load_preset_info(origin, relative_file_path, file_type, "", &file_content)
    }

    pub fn add_preset(&mut self, preset: CompartmentPresetModel) -> anyhow::Result<()> {
        let path = self.preset_dir_path.join(format!("{}.json", preset.id()));
        fs::create_dir_all(path.parent().context("impossible")?)
            .context("couldn't create preset directory")?;
        let mut data = CompartmentPresetData::from_model(&preset);
        // We don't want to have the ID in the file - because the file name itself is the ID
        data.clear_id();
        let json = serde_json::to_string_pretty(&data).context("couldn't serialize preset")?;
        fs::write(path, json).context("couldn't write preset file")?;
        let _ = self.load_presets_from_disk();
        Ok(())
    }

    pub fn update_preset(&mut self, preset: CompartmentPresetModel) -> anyhow::Result<()> {
        self.add_preset(preset)
    }

    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.changed_subject.clone()
    }

    pub fn log_debug_info(&self) {
        let msg = format!(
            "\n\
            # Preset manager\n\
            \n\
            - Preset count: {}\n\
            ",
            self.preset_infos.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    pub fn find_preset_info_by_id(&self, id: &str) -> Option<&PresetInfo<S>> {
        self.preset_infos.iter().find(|info| &info.common.id == id)
    }

    pub fn preset_infos(&self) -> &[PresetInfo<S>] {
        &self.preset_infos
    }

    fn notify_presets_changed(&mut self) {
        self.event_handler.presets_changed(self);
        self.changed_subject.next(());
    }

    fn load_full_preset(
        &self,
        preset_info: &PresetInfo<S>,
    ) -> anyhow::Result<CompartmentPresetModel> {
        let file_content: Cow<str> = match &preset_info.common.origin {
            PresetOrigin::User {
                absolute_file_path: absolute_path,
            } => fs::read_to_string(absolute_path)
                .map_err(|_| {
                    anyhow!(
                        "Couldn't read preset file \"{}\" anymore.",
                        &preset_info.common.origin
                    )
                })?
                .into(),
            PresetOrigin::Factory {
                compartment,
                relative_file_path,
            } => get_factory_preset_content(*compartment, relative_file_path)?.into(),
        };
        match preset_info.common.file_type {
            PresetFileType::Json => {
                let data: CompartmentPresetData =
                    serde_json::from_str(&file_content).map_err(|e| {
                        anyhow!(
                            "Preset file {} isn't valid anymore. Details:\n\n{}",
                            &preset_info.common.origin,
                            e
                        )
                    })?;
                data.to_model(preset_info.common.id.clone(), self.compartment)
            }
            PresetFileType::Lua => {
                let lua = SafeLua::new()?;
                let lua = lua.start_execution_time_limit_countdown()?;
                let env = lua.create_fresh_environment(true)?;
                if preset_info.common.origin.is_factory() {
                    let factory_presets_dir = get_factory_preset_dir(self.compartment).clone();
                    let mut module_container = LuaModuleContainer::new(
                        IncludedDirLuaModuleFinder::new(factory_presets_dir),
                    );
                    module_container.install_to(&env, lua.as_ref())?;
                }
                let value = lua.compile_and_execute(
                    preset_info.common.origin.to_string().as_ref(),
                    &file_content,
                    env,
                )?;
                let compartment_content: realearn_api::persistence::CompartmentContent =
                    lua.as_ref().from_value(value)?;
                let compartment_data = convert_compartment(self.compartment, compartment_content)?;
                let compartment_model = compartment_data.to_model(
                    preset_info.common.meta_data.realearn_version.as_ref(),
                    self.compartment,
                    None,
                )?;
                let preset_model = CompartmentPresetModel::new(
                    preset_info.common.id.to_string(),
                    preset_info.common.meta_data.name.to_string(),
                    self.compartment,
                    compartment_model,
                );
                Ok(preset_model)
            }
        }
    }
}

fn get_factory_preset_content(
    compartment: Compartment,
    relative_file_path: &Path,
) -> anyhow::Result<&'static str> {
    let factory_preset_dir = get_factory_preset_dir(compartment);
    let file = factory_preset_dir
        .get_file(relative_file_path)
        .context("Couldn't find factory preset anymore")?;
    file.contents_utf8()
        .context("Factory preset not UTF-8 anymore!?")
}

impl<M: SpecificPresetMetaData> CommonCompartmentPresetManager
    for FileBasedCompartmentPresetManager<M>
{
    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()> {
        let preset_info = self
            .preset_infos
            .iter()
            .find(|info| info.common.id == id)
            .context("preset to be removed not found")?;
        let PresetOrigin::User {
            absolute_file_path: absolute_path,
        } = &preset_info.common.origin
        else {
            bail!("can't delete factory presets");
        };
        fs::remove_file(absolute_path).context("couldn't delete preset file")?;
        let _ = self.load_presets_from_disk();
        Ok(())
    }

    fn common_preset_infos(&self) -> Box<dyn Iterator<Item = &CommonPresetInfo> + '_> {
        Box::new(self.preset_infos.iter().map(|info| &info.common))
    }

    fn common_preset_info_by_id(&self, id: &str) -> Option<&CommonPresetInfo> {
        self.preset_infos
            .iter()
            .find(|info| &info.common.id == id)
            .map(|info| &info.common)
    }

    fn save_original_factory_preset_as_user_preset(
        &mut self,
        relative_file_path: &Path,
    ) -> anyhow::Result<String> {
        let relative_user_file_path = PathBuf::from(whoami::username()).join(relative_file_path);
        let dest_file_path = self.preset_dir_path.join(&relative_user_file_path);
        let preset_content = get_factory_preset_content(self.compartment, relative_file_path)?;
        fs::create_dir_all(dest_file_path.parent().context("impossible")?)?;
        fs::write(&dest_file_path, preset_content)?;
        let _ = self.load_presets_from_disk();
        let _ = open_in_file_manager(&dest_file_path);
        let origin = PresetOrigin::User {
            absolute_file_path: dest_file_path.clone(),
        };
        build_id(&relative_user_file_path, "", &origin)
    }
}

impl<M: SpecificPresetMetaData> CompartmentPresetManager for FileBasedCompartmentPresetManager<M> {
    fn find_by_id(&self, id: &str) -> Option<CompartmentPresetModel> {
        let preset_info = self.preset_infos.iter().find(|info| info.common.id == id)?;
        match self.load_full_preset(preset_info) {
            Ok(p) => Some(p),
            Err(e) => {
                notification::warn(e.to_string());
                None
            }
        }
    }
}

impl<T: CompartmentPresetManager> CompartmentPresetManager for Rc<RefCell<T>> {
    fn find_by_id(&self, id: &str) -> Option<CompartmentPresetModel> {
        self.borrow().find_by_id(id)
    }
}

pub static FACTORY_CONTROLLER_PRESETS_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../resources/controller-presets/factory");
pub static FACTORY_MAIN_PRESETS_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../resources/main-presets/factory");

fn walk_included_dir(
    dir: &Dir,
    on_file: &mut impl FnMut(&include_dir::File) -> anyhow::Result<()>,
) {
    use include_dir::DirEntry;
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(dir) => {
                walk_included_dir(dir, on_file);
            }
            DirEntry::File(file) => {
                warn_user_on_anyhow_error(on_file(file));
            }
        }
    }
}

/// Returns `None` if the file is in the preset folder but not a preset, e.g. a Lua file which
/// will be required by presets.
fn load_preset_info<M: SpecificPresetMetaData>(
    origin: PresetOrigin,
    relative_file_path: &Path,
    file_type: PresetFileType,
    id_prefix: &str,
    file_content: &str,
) -> anyhow::Result<PresetInfo<M>> {
    let id = build_id(relative_file_path, id_prefix, &origin)?;
    let preset_meta_data_result = match file_type {
        PresetFileType::Json => CombinedPresetMetaData::from_json(&file_content),
        PresetFileType::Lua => CombinedPresetMetaData::from_lua_code(&file_content),
    };
    let preset_meta_data = preset_meta_data_result.map_err(|e| {
        anyhow!("Couldn't read preset meta data from \"{origin}\". Details:\n\n{e}",)
    })?;
    if let Some(v) = preset_meta_data.common.realearn_version.as_ref() {
        if BackboneShell::version() < v {
            bail!(
                "Skipped loading of preset \"{origin}\" because it has been created with \
                         ReaLearn {v}, which is newer than the installed version {}. \
                         Please update your ReaLearn version.",
                BackboneShell::version()
            );
        }
    }
    let preset_info = PresetInfo {
        common: CommonPresetInfo {
            id,
            file_type,
            origin,
            meta_data: preset_meta_data.common,
        },
        specific_meta_data: preset_meta_data.specific,
    };
    Ok(preset_info)
}

fn build_id(
    relative_file_path: &Path,
    prefix: &str,
    origin: &PresetOrigin,
) -> anyhow::Result<String> {
    let file_stem = relative_file_path.file_stem().ok_or_else(|| {
        anyhow!(
            "Preset file \"{origin}\" only has an extension but not a name. \
                    The name is necessary because it makes up the preset ID.",
        )
    })?;
    let leaf_id = file_stem.to_string_lossy();
    let relative_dir_path = relative_file_path
        .parent()
        .context("relative file was a dir actually")?;
    let id = if relative_dir_path.parent().is_none() {
        // Preset is in root
        format!("{prefix}{leaf_id}")
    } else {
        // Preset is in sub directory
        let relative_dir_path_with_slashes = relative_dir_path.to_string_lossy().replace('\\', "/");
        format!("{prefix}{relative_dir_path_with_slashes}/{leaf_id}")
    };
    Ok(id)
}

pub fn get_factory_preset_dir(compartment: Compartment) -> &'static Dir<'static> {
    match compartment {
        Compartment::Controller => &FACTORY_CONTROLLER_PRESETS_DIR,
        Compartment::Main => &FACTORY_MAIN_PRESETS_DIR,
    }
}

pub fn parse_lua_frontmatter<T: for<'a> Deserialize<'a>>(lua_code: &str) -> anyhow::Result<T> {
    const PREFIX: &str = "--- ";
    let frontmatter = lua_code
        .lines()
        .map_while(|line| line.strip_prefix(PREFIX))
        .join("\n");
    if frontmatter.is_empty() {
        bail!("Lua presets need at least a \"{PREFIX} name: ...\" line at the very top!");
    }
    let value: T = serde_yaml::from_str(&frontmatter).map_err(|e| {
        anyhow!("Error while parsing Lua preset frontmatter:\n\n{e}\n\nFrontmatter was:\n===\n{frontmatter}\n===\n")
    })?;
    Ok(value)
}
