use crate::application::{
    GroupModel, MappingModel, ParameterSetting, Preset, PresetManager, SharedGroup, SharedMapping,
};
use crate::infrastructure::data::{GroupModelData, MappingModelData};

use crate::core::notification;
use reaper_high::Reaper;
use rxrust::prelude::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FileBasedPresetManager<P: Preset, PD: PresetData<P = P>> {
    preset_dir_path: PathBuf,
    presets: Vec<P>,
    changed_subject: LocalSubject<'static, (), ()>,
    p: PhantomData<PD>,
}

pub trait ExtendedPresetManager {
    fn find_index_by_id(&self, id: &str) -> Option<usize>;
    fn find_id_by_index(&self, index: usize) -> Option<String>;
    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str>;
}

impl<P: Preset, PD: PresetData<P = P>> FileBasedPresetManager<P, PD> {
    pub fn new(preset_dir_path: PathBuf) -> FileBasedPresetManager<P, PD> {
        let mut manager = FileBasedPresetManager {
            preset_dir_path,
            presets: vec![],
            changed_subject: Default::default(),
            p: PhantomData,
        };
        let _ = manager.load_presets_internal();
        manager
    }

    pub fn load_presets(&mut self) -> Result<(), String> {
        self.load_presets_internal()?;
        self.notify_changed();
        Ok(())
    }

    fn load_presets_internal(&mut self) -> Result<(), String> {
        let preset_file_paths = fs::read_dir(&self.preset_dir_path)
            .map_err(|_| "couldn't read preset directory".to_string())?
            .filter_map(|result| {
                let dir_entry = result.ok()?;
                let file_type = dir_entry.file_type().ok()?;
                if !file_type.is_file() {
                    return None;
                }
                let path = dir_entry.path();
                if path.extension() != Some(std::ffi::OsStr::new("json")) {
                    return None;
                };
                Some(path)
            });
        self.presets = preset_file_paths
            .filter_map(|p| Self::load_preset(p).ok())
            .collect();
        self.presets
            .sort_unstable_by_key(|p| p.name().to_lowercase());
        Ok(())
    }

    pub fn presets(&self) -> impl Iterator<Item = &P> + ExactSizeIterator {
        self.presets.iter()
    }

    pub fn find_by_index(&self, index: usize) -> Option<&P> {
        self.presets.get(index)
    }

    pub fn add_preset(&mut self, preset: P) -> Result<(), &'static str> {
        let path = self.get_preset_file_path(preset.id());
        fs::create_dir_all(&self.preset_dir_path)
            .map_err(|_| "couldn't create preset directory")?;
        let mut data = PD::from_model(&preset);
        // We don't want to have the ID in the file - because the file name itself is the ID
        data.clear_id();
        let json = serde_json::to_string_pretty(&data).map_err(|_| "couldn't serialize preset")?;
        fs::write(path, json).map_err(|_| "couldn't write preset file")?;
        let _ = self.load_presets();
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
            self.presets.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    fn notify_changed(&mut self) {
        self.changed_subject.next(());
    }

    fn get_preset_file_path(&self, id: &str) -> PathBuf {
        self.preset_dir_path.join(format!("{}.json", id))
    }

    fn load_preset(path: impl AsRef<Path>) -> Result<P, String> {
        let id = path
            .as_ref()
            .file_stem()
            .ok_or_else(|| "preset file must have stem because it makes up the ID".to_string())?
            .to_string_lossy()
            .to_string();
        let json =
            fs::read_to_string(&path).map_err(|_| "couldn't read preset file".to_string())?;
        let data: PD = serde_json::from_str(&json).map_err(|e| {
            format!(
                "Preset file {:?} isn't valid. Details:\n\n{}",
                path.as_ref(),
                e
            )
        })?;
        if data.was_saved_with_newer_version() {
            notification::warn(
                "The preset that is about to load was saved with a newer version of ReaLearn. Things might not work as expected. Even more importantly: Saving the preset might result in loss of the data that was saved with the new ReaLearn version! Please consider upgrading your ReaLearn installation to the latest version.",
            );
        }
        Ok(data.to_model(id))
    }

    fn find_preset_ref_by_id(&self, id: &str) -> Option<&P> {
        self.presets.iter().find(|c| c.id() == id)
    }
}

impl<P: Preset, PD: PresetData<P = P>> ExtendedPresetManager for FileBasedPresetManager<P, PD> {
    fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.presets.iter().position(|p| p.id() == id)
    }

    fn find_id_by_index(&self, index: usize) -> Option<String> {
        let preset = self.find_by_index(index)?;
        Some(preset.id().to_string())
    }

    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str> {
        let path = self.get_preset_file_path(id);
        fs::remove_file(path).map_err(|_| "couldn't delete preset file")?;
        let _ = self.load_presets();
        Ok(())
    }
}

impl<P: Preset, PD: PresetData<P = P>> PresetManager for FileBasedPresetManager<P, PD> {
    type PresetType = P;

    fn find_by_id(&self, id: &str) -> Option<P> {
        self.presets.iter().find(|c| c.id() == id).cloned()
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        let preset = match self.find_preset_ref_by_id(id) {
            None => return false,
            Some(c) => c,
        };
        if mappings.len() != preset.mappings().len() {
            return true;
        }
        mappings
            .iter()
            .zip(preset.mappings().iter())
            .any(|(actual_mapping, preset_mapping)| {
                !mappings_are_equal(&actual_mapping.borrow(), preset_mapping)
            })
    }

    fn parameter_settings_are_dirty(
        &self,
        id: &str,
        parameter_settings: &HashMap<u32, ParameterSetting>,
    ) -> bool {
        let preset = match self.find_preset_ref_by_id(id) {
            None => return false,
            Some(c) => c,
        };
        parameter_settings != preset.parameters()
    }

    fn groups_are_dirty(
        &self,
        id: &str,
        default_group: &SharedGroup,
        groups: &[SharedGroup],
    ) -> bool {
        let preset = match self.find_preset_ref_by_id(id) {
            None => return false,
            Some(c) => c,
        };
        if groups.len() != preset.groups().len() {
            return true;
        }
        if !groups_are_equal(&default_group.borrow(), preset.default_group()) {
            return true;
        }
        groups
            .iter()
            .zip(preset.groups().iter())
            .any(|(actual_group, preset_group)| {
                !groups_are_equal(&actual_group.borrow(), preset_group)
            })
    }
}

fn groups_are_equal(first: &GroupModel, second: &GroupModel) -> bool {
    let first_data = GroupModelData::from_model(first);
    let second_data = GroupModelData::from_model(second);
    first_data == second_data
}

fn mappings_are_equal(first: &MappingModel, second: &MappingModel) -> bool {
    let first_data = MappingModelData::from_model(first);
    let second_data = MappingModelData::from_model(second);
    first_data == second_data
}

pub trait PresetData: Sized + Serialize + DeserializeOwned + Debug {
    type P: Preset;

    fn from_model(preset: &Self::P) -> Self;

    fn to_model(&self, id: String) -> Self::P;

    fn clear_id(&mut self);

    fn was_saved_with_newer_version(&self) -> bool;
}
