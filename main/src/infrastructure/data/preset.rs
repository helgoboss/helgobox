use crate::application::{Preset, PresetManager};
use base::default_util::deserialize_null_default;

use crate::base::notification;
use crate::domain::{Compartment, SafeLua};
use crate::infrastructure::api::convert::to_data::convert_compartment;
use crate::infrastructure::plugin::BackboneShell;
use anyhow::{anyhow, bail, ensure, Context};
use base::file_util;
use include_dir::{include_dir, Dir};
use mlua::LuaSerdeExt;
use reaper_high::Reaper;
use rxrust::prelude::*;
use semver::Version;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fmt, fs};
use strum::EnumIs;
use walkdir::WalkDir;

#[derive(Debug)]
pub struct FileBasedPresetManager<P: Preset, PD: PresetData<P = P>> {
    preset_dir_path: PathBuf,
    preset_infos: Vec<PresetInfo>,
    changed_subject: LocalSubject<'static, (), ()>,
    event_handler: Box<dyn PresetManagerEventHandler<Source = Self>>,
    p: PhantomData<PD>,
}

pub trait PresetManagerEventHandler: fmt::Debug {
    type Source;

    fn presets_changed(&self, source: &Self::Source);
}

pub trait ExtendedPresetManager {
    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()>;
    fn preset_infos(&self) -> &[PresetInfo];
    fn preset_info_by_id(&self, id: &str) -> Option<&PresetInfo>;
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct PresetInfo {
    pub id: String,
    pub realearn_version: Option<Version>,
    pub name: String,
    pub file_type: PresetFileType,
    pub origin: PresetOrigin,
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

#[derive(Clone, Eq, PartialEq, Debug, Default, Deserialize)]
struct PresetMetaData {
    pub name: String,
    // Since ReaLearn 1.12.0-pre18
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    #[serde(alias = "version")]
    pub realearn_version: Option<Version>,
}

impl PresetMetaData {
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn from_lua(lua: &str) -> anyhow::Result<Self> {
        let mut md = PresetMetaData::default();
        const PREFIX: &str = "--- ";
        for line in lua.lines() {
            let Some(meta_data_pair) = line.strip_prefix(PREFIX) else {
                break;
            };
            if let Some(name) = meta_data_pair.strip_prefix("name: ") {
                md.name = name.trim().to_string();
            }
            if let Some(realearn_version) = meta_data_pair.strip_prefix("realearn_version: ") {
                md.realearn_version = Some(Version::parse(realearn_version.trim())?);
            }
        }
        ensure!(
            !md.name.is_empty(),
            "Lua presets need at least a \"{PREFIX}name: ...\" line at the very top!"
        );
        Ok(md)
    }
}

impl<P: Preset, PD: PresetData<P = P>> FileBasedPresetManager<P, PD> {
    pub fn new(
        preset_dir_path: PathBuf,
        event_handler: Box<dyn PresetManagerEventHandler<Source = Self>>,
    ) -> FileBasedPresetManager<P, PD> {
        let mut manager = FileBasedPresetManager {
            preset_dir_path,
            preset_infos: vec![],
            changed_subject: Default::default(),
            event_handler,
            p: PhantomData,
        };
        let _ = manager.load_preset_infos_internal();
        manager
    }

    pub fn load_preset_infos(&mut self) -> Result<(), String> {
        self.load_preset_infos_internal()?;
        self.notify_presets_changed();
        Ok(())
    }

    fn load_preset_infos_internal(&mut self) -> Result<(), String> {
        // Load factory preset infos
        let compartment = P::compartment();
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
            .max_depth(2)
            .into_iter()
            .filter_entry(|e| !file_util::is_hidden(e))
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                let file_type = PresetFileType::from_path(entry.path())?;
                match self.build_user_preset_info(entry, file_type) {
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
        entry: walkdir::DirEntry,
        file_type: PresetFileType,
    ) -> anyhow::Result<PresetInfo> {
        let absolute_path = entry.into_path();
        let origin = PresetOrigin::User {
            absolute_file_path: absolute_path.clone(),
        };
        let relative_file_path = absolute_path.strip_prefix(&self.preset_dir_path).unwrap();
        let file_content = fs::read_to_string(&absolute_path)
            .map_err(|_| anyhow!("Couldn't read preset file \"{}\".", absolute_path.display()))?;
        load_preset_info(origin, relative_file_path, file_type, "", &file_content)
    }

    pub fn add_preset(&mut self, preset: P) -> Result<(), &'static str> {
        let path = self.preset_dir_path.join(format!("{}.json", preset.id()));
        fs::create_dir_all(&self.preset_dir_path)
            .map_err(|_| "couldn't create preset directory")?;
        let mut data = PD::from_model(&preset);
        // We don't want to have the ID in the file - because the file name itself is the ID
        data.clear_id();
        let json = serde_json::to_string_pretty(&data).map_err(|_| "couldn't serialize preset")?;
        fs::write(path, json).map_err(|_| "couldn't write preset file")?;
        let _ = self.load_preset_infos();
        Ok(())
    }

    pub fn update_preset(&mut self, preset: P) -> Result<(), &'static str> {
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

    fn notify_presets_changed(&mut self) {
        self.event_handler.presets_changed(self);
        self.changed_subject.next(());
    }

    fn load_full_preset(&self, preset_info: &PresetInfo) -> anyhow::Result<P> {
        let file_content: Cow<str> = match &preset_info.origin {
            PresetOrigin::User {
                absolute_file_path: absolute_path,
            } => fs::read_to_string(absolute_path)
                .map_err(|_| {
                    anyhow!(
                        "Couldn't read preset file \"{}\" anymore.",
                        &preset_info.origin
                    )
                })?
                .into(),
            PresetOrigin::Factory {
                compartment,
                relative_file_path: relative_path,
            } => {
                let factory_preset_dir = get_factory_preset_dir(*compartment);
                let file = factory_preset_dir
                    .get_file(relative_path)
                    .context("Couldn't find factory preset anymore")?;
                file.contents_utf8()
                    .context("Factory preset not UTF-8 anymore!?")?
                    .into()
            }
        };
        match preset_info.file_type {
            PresetFileType::Json => {
                let data: PD = serde_json::from_str(&file_content).map_err(|e| {
                    anyhow!(
                        "Preset file {} isn't valid anymore. Details:\n\n{}",
                        &preset_info.origin,
                        e
                    )
                })?;
                data.to_model(preset_info.id.clone())
            }
            PresetFileType::Lua => {
                let lua = SafeLua::new()?;
                let lua = lua.start_execution_time_limit_countdown(Duration::from_millis(200))?;
                let env = lua.create_fresh_environment(true)?;
                let value = lua.compile_and_execute(
                    preset_info.origin.to_string().as_ref(),
                    &file_content,
                    env,
                )?;
                let api_compartment: realearn_api::persistence::Compartment =
                    lua.as_ref().from_value(value)?;
                let compartment_data = convert_compartment(api_compartment)?;
                let compartment_model = compartment_data.to_model(
                    preset_info.realearn_version.as_ref(),
                    P::compartment(),
                    None,
                )?;
                let preset_model = P::from_parts(
                    preset_info.id.to_string(),
                    preset_info.name.to_string(),
                    compartment_model,
                );
                Ok(preset_model)
            }
        }
    }
}

impl<P: Preset, PD: PresetData<P = P>> ExtendedPresetManager for FileBasedPresetManager<P, PD> {
    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()> {
        let preset_info = self
            .preset_infos
            .iter()
            .find(|info| info.id == id)
            .context("preset to be removed not found")?;
        let PresetOrigin::User {
            absolute_file_path: absolute_path,
        } = &preset_info.origin
        else {
            bail!("can't delete factory presets");
        };
        fs::remove_file(absolute_path).context("couldn't delete preset file")?;
        let _ = self.load_preset_infos();
        Ok(())
    }

    fn preset_infos(&self) -> &[PresetInfo] {
        &self.preset_infos
    }

    fn preset_info_by_id(&self, id: &str) -> Option<&PresetInfo> {
        self.preset_infos.iter().find(|info| &info.id == id)
    }
}

impl<P: Preset + Clone, PD: PresetData<P = P>> PresetManager for FileBasedPresetManager<P, PD> {
    type PresetType = P;

    fn find_by_id(&self, id: &str) -> Option<P> {
        let preset_info = self.preset_infos.iter().find(|info| info.id == id)?;
        match self.load_full_preset(preset_info) {
            Ok(p) => Some(p),
            Err(e) => {
                notification::warn(e.to_string());
                None
            }
        }
    }
}

pub trait PresetData: Sized + Serialize + DeserializeOwned + fmt::Debug {
    type P: Preset;

    fn from_model(preset: &Self::P) -> Self;

    fn to_model(&self, id: String) -> anyhow::Result<Self::P>;

    fn clear_id(&mut self);

    fn version(&self) -> Option<&Version>;
}

static FACTORY_CONTROLLER_PRESETS_DIR: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../resources/controller-presets/factory");
static FACTORY_MAIN_PRESETS_DIR: Dir<'_> =
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
                on_file(file).unwrap();
            }
        }
    }
}

fn load_preset_info(
    origin: PresetOrigin,
    relative_file_path: &Path,
    file_type: PresetFileType,
    id_prefix: &str,
    file_content: &str,
) -> anyhow::Result<PresetInfo> {
    let id = build_id(relative_file_path, id_prefix, &origin)?;
    let preset_meta_data_result = match file_type {
        PresetFileType::Json => PresetMetaData::from_json(&file_content),
        PresetFileType::Lua => PresetMetaData::from_lua(&file_content),
    };
    let preset_meta_data = preset_meta_data_result.map_err(|e| {
        anyhow!("Couldn't read preset meta data from \"{origin}\". Details:\n\n{e}",)
    })?;
    if let Some(v) = preset_meta_data.realearn_version.as_ref() {
        if BackboneShell::version() < v {
            bail!(
                "Skipped loading of preset \"{origin}\" because it has been created with \
                         ReaLearn {v}, which is newer than the installed version {}. \
                         Please update your ReaLearn version. If this is not an option for you and \
                         it's a factory preset installed from ReaPack, go back to an older version \
                         of that preset and pin it so that future ReaPack synchronization won't \
                         automatically update that preset. Alternatively, make your own copy of \
                         the preset and uninstall the factory preset.",
                BackboneShell::version()
            );
        }
    }
    let preset_info = PresetInfo {
        id,
        realearn_version: preset_meta_data.realearn_version,
        name: preset_meta_data.name,
        file_type,
        origin,
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

fn get_factory_preset_dir(compartment: Compartment) -> &'static Dir<'static> {
    match compartment {
        Compartment::Controller => &FACTORY_CONTROLLER_PRESETS_DIR,
        Compartment::Main => &FACTORY_MAIN_PRESETS_DIR,
    }
}
