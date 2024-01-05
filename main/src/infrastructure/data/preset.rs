use crate::application::{Preset, PresetManager};
use base::default_util::deserialize_null_default;
use std::error::Error;

use crate::base::notification;
use crate::infrastructure::plugin::BackboneShell;
use anyhow::{anyhow, bail, Context};
use base::file_util;
use reaper_high::Reaper;
use rxrust::prelude::*;
use semver::Version;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs;
use std::marker::PhantomData;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug)]
pub struct FileBasedPresetManager<P: Preset, PD: PresetData<P = P>> {
    preset_dir_path: PathBuf,
    preset_infos: Vec<PresetInfo>,
    changed_subject: LocalSubject<'static, (), ()>,
    event_handler: Box<dyn PresetManagerEventHandler<Source = Self>>,
    p: PhantomData<PD>,
}

pub trait PresetManagerEventHandler: Debug {
    type Source;

    fn presets_changed(&self, source: &Self::Source);
}

pub trait ExtendedPresetManager {
    fn exists(&self, id: &str) -> bool {
        self.find_index_by_id(id).is_some()
    }
    fn find_index_by_id(&self, id: &str) -> Option<usize>;
    fn find_id_by_index(&self, index: usize) -> Option<String>;
    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()>;
    fn preset_infos(&self) -> &[PresetInfo];
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    /// The ID should actually be equal to the path, but to not run into any
    /// breaking change in edge cases, we keep track of the actual path here.
    pub absolute_path: PathBuf,
}

#[derive(Clone, Eq, PartialEq, Debug, Deserialize)]
pub struct PresetInfoData {
    pub name: String,
    // Since ReaLearn 1.12.0-pre18
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub version: Option<Version>,
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
        let preset_file_paths = WalkDir::new(&self.preset_dir_path)
            .follow_links(true)
            .max_depth(2)
            .into_iter()
            .filter_entry(|e| !file_util::is_hidden(e))
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().is_file() {
                    return None;
                }
                if entry.path().extension() != Some(std::ffi::OsStr::new("json")) {
                    return None;
                }
                Some(entry.into_path())
            });
        self.preset_infos = preset_file_paths
            .filter_map(|p| match self.load_preset_info(p) {
                Ok(p) => Some(p),
                Err(e) => {
                    notification::warn(e.to_string());
                    None
                }
            })
            .collect();
        self.preset_infos
            .sort_unstable_by_key(|p| p.name.to_lowercase());
        Ok(())
    }

    pub fn find_preset_info_by_index(&self, index: usize) -> Option<&PresetInfo> {
        self.preset_infos.get(index)
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

    fn load_preset_info(&self, path: PathBuf) -> anyhow::Result<PresetInfo> {
        let relative_path = path
            .parent()
            .unwrap()
            .strip_prefix(&self.preset_dir_path)
            .unwrap();
        let file_stem = path.file_stem().ok_or_else(|| {
            anyhow!(
                "Preset file \"{}\" only has an extension but not a name. \
                    The name is necessary because it makes up the preset ID.",
                path.display()
            )
        })?;
        let leaf_id = file_stem.to_string_lossy();
        let id = if relative_path.parent().is_none() {
            // Preset is in root
            leaf_id.to_string()
        } else {
            // Preset is in sub directory
            let relative_path_with_slashes = relative_path.to_string_lossy().replace('\\', "/");
            format!("{relative_path_with_slashes}/{leaf_id}")
        };
        let json = fs::read_to_string(&path)
            .map_err(|_| anyhow!("Couldn't read preset file \"{}\".", path.display()))?;
        let data: PresetInfoData = serde_json::from_str(&json).map_err(|e| {
            anyhow!(
                "Preset file {} isn't valid. Details:\n\n{}",
                path.display(),
                e
            )
        })?;
        if let Some(v) = data.version.as_ref() {
            if BackboneShell::version() < v {
                bail!(
                    "Skipped loading of preset \"{}\" because it has been saved with \
                         ReaLearn {}, which is newer than the installed version {}. \
                         Please update your ReaLearn version. If this is not an option for you and \
                         it's a factory preset installed from ReaPack, go back to an older version \
                         of that preset and pin it so that future ReaPack synchronization won't \
                         automatically update that preset. Alternatively, make your own copy of \
                         the preset and uninstall the factory preset.",
                    path.display(),
                    v,
                    BackboneShell::version()
                );
            }
        }
        let preset_info = PresetInfo {
            id,
            name: data.name,
            absolute_path: path,
        };
        Ok(preset_info)
    }

    fn load_full_preset(&self, preset_info: &PresetInfo) -> Result<P, Box<dyn Error>> {
        let json = fs::read_to_string(&preset_info.absolute_path).map_err(|_| {
            format!(
                "Couldn't read preset file \"{}\" anymore.",
                preset_info.absolute_path.display()
            )
        })?;
        let data: PD = serde_json::from_str(&json).map_err(|e| {
            format!(
                "Preset file {} isn't valid anymore. Details:\n\n{}",
                preset_info.absolute_path.display(),
                e
            )
        })?;
        data.to_model(preset_info.id.clone())
    }
}

impl<P: Preset, PD: PresetData<P = P>> ExtendedPresetManager for FileBasedPresetManager<P, PD> {
    fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.preset_infos.iter().position(|p| &p.id == id)
    }

    fn find_id_by_index(&self, index: usize) -> Option<String> {
        let preset_info = self.find_preset_info_by_index(index)?;
        Some(preset_info.id.clone())
    }

    fn remove_preset(&mut self, id: &str) -> anyhow::Result<()> {
        let preset_info = self
            .preset_infos
            .iter()
            .find(|info| info.id == id)
            .context("preset to be removed not found")?;
        fs::remove_file(&preset_info.absolute_path).context("couldn't delete preset file")?;
        let _ = self.load_preset_infos();
        Ok(())
    }

    fn preset_infos(&self) -> &[PresetInfo] {
        &self.preset_infos
    }
}

impl<P: Preset + Clone, PD: PresetData<P = P>> PresetManager for FileBasedPresetManager<P, PD> {
    type PresetType = P;

    fn find_by_id(&self, id: &str) -> Option<P> {
        let preset_info = self.preset_infos.iter().find(|info| info.id == id)?;
        self.load_full_preset(preset_info).ok()
    }
}

pub trait PresetData: Sized + Serialize + DeserializeOwned + Debug {
    type P: Preset;

    fn from_model(preset: &Self::P) -> Self;

    fn to_model(&self, id: String) -> Result<Self::P, Box<dyn Error>>;

    fn clear_id(&mut self);

    fn version(&self) -> Option<&Version>;
}
