use crate::application::{Preset, PresetManager};

use crate::base::{file_util, notification};
use crate::infrastructure::plugin::App;
use reaper_high::Reaper;
use rxrust::prelude::*;
use semver::Version;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct FileBasedPresetManager<P: Preset, PD: PresetData<P = P>> {
    preset_dir_path: PathBuf,
    presets: Vec<P>,
    changed_subject: LocalSubject<'static, (), ()>,
    p: PhantomData<PD>,
}

pub trait ExtendedPresetManager {
    fn exists(&self, id: &str) -> bool {
        self.find_index_by_id(id).is_some()
    }
    fn find_index_by_id(&self, id: &str) -> Option<usize>;
    fn find_id_by_index(&self, index: usize) -> Option<String>;
    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str>;
    fn preset_infos(&self) -> Vec<PresetInfo>;
}

pub struct PresetInfo {
    pub id: String,
    pub name: String,
}

impl<P: Preset, PD: PresetData<P = P>> FileBasedPresetManager<P, PD> {
    pub fn new(preset_dir_path: PathBuf) -> FileBasedPresetManager<P, PD> {
        let mut manager = FileBasedPresetManager {
            preset_dir_path,
            presets: vec![],
            changed_subject: Default::default(),
            p: PhantomData,
        };
        // Pre-loading all presets used to take lots of memory when we still used Rx Props, around
        // 70 MB with my preset collection. But now the same takes just 5 MB, so this alone is not
        // an urgent reason anymore to move to lazy preset loading.
        let _ = manager.load_presets_internal();
        manager
    }

    pub fn load_presets(&mut self) -> Result<(), String> {
        self.load_presets_internal()?;
        self.notify_changed();
        Ok(())
    }

    fn load_presets_internal(&mut self) -> Result<(), String> {
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
        self.presets = preset_file_paths
            .filter_map(|p| match self.load_preset(&p) {
                Ok(p) => Some(p),
                Err(msg) => {
                    notification::warn(msg);
                    None
                }
            })
            .collect();
        self.presets
            .sort_unstable_by_key(|p| p.name().to_lowercase());
        Ok(())
    }

    pub fn preset_iter(&self) -> impl Iterator<Item = &P> + ExactSizeIterator {
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
        self.preset_dir_path.join(format!("{id}.json"))
    }

    fn load_preset(&self, path: &Path) -> Result<P, String> {
        let relative_path = path
            .parent()
            .unwrap()
            .strip_prefix(&self.preset_dir_path)
            .unwrap();
        let file_stem = path.file_stem().ok_or_else(|| {
            format!(
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
        let json = fs::read_to_string(path)
            .map_err(|_| format!("Couldn't read preset file \"{}\".", path.display()))?;
        let data: PD = serde_json::from_str(&json).map_err(|e| {
            format!(
                "Preset file {} isn't valid. Details:\n\n{}",
                path.display(),
                e
            )
        })?;
        if let Some(v) = data.version() {
            if App::version() < v {
                let msg = format!(
                    "Skipped loading of preset \"{}\" because it has been saved with \
                         ReaLearn {}, which is newer than the installed version {}. \
                         Please update your ReaLearn version. If this is not an option for you and \
                         it's a factory preset installed from ReaPack, go back to an older version \
                         of that preset and pin it so that future ReaPack synchronization won't \
                         automatically update that preset. Alternatively, make your own copy of \
                         the preset and uninstall the factory preset.",
                    path.display(),
                    v,
                    App::version()
                );
                return Err(msg);
            }
        }
        data.to_model(id)
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

    fn preset_infos(&self) -> Vec<PresetInfo> {
        self.presets
            .iter()
            .map(|p| PresetInfo {
                id: p.id().to_owned(),
                name: p.name().to_owned(),
            })
            .collect()
    }
}

impl<P: Preset + Clone, PD: PresetData<P = P>> PresetManager for FileBasedPresetManager<P, PD> {
    type PresetType = P;

    fn find_by_id(&self, id: &str) -> Option<P> {
        self.presets.iter().find(|c| c.id() == id).cloned()
    }
}

pub trait PresetData: Sized + Serialize + DeserializeOwned + Debug {
    type P: Preset;

    fn from_model(preset: &Self::P) -> Self;

    fn to_model(&self, id: String) -> Result<Self::P, String>;

    fn clear_id(&mut self);

    fn version(&self) -> Option<&Version>;
}
