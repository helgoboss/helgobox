use crate::application::{Preset, PresetManager};

use crate::base::notification;
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

#[derive(Debug)]
pub struct FileBasedPresetManager<P: Preset, PD: PresetData<P = P>> {
    preset_dir_path: PathBuf,
    preset_descriptors: Vec<PresetDescriptor>,
    changed_subject: LocalSubject<'static, (), ()>,
    p: PhantomData<PD>,
}

#[derive(Debug)]
pub struct PresetDescriptor {
    pub id: String,
    pub name: String,
}

pub trait ExtendedPresetManager {
    fn find_index_by_id(&self, id: &str) -> Option<usize>;
    fn find_id_by_index(&self, index: usize) -> Option<String>;
    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str>;
}

// TODO-high P can be eliminated because it's associated type of PD
impl<P: Preset, PD: PresetData<P = P>> FileBasedPresetManager<P, PD> {
    pub fn new(preset_dir_path: PathBuf) -> FileBasedPresetManager<P, PD> {
        let mut manager = FileBasedPresetManager {
            preset_dir_path,
            preset_descriptors: vec![],
            changed_subject: Default::default(),
            p: PhantomData,
        };
        let _ = manager.load_preset_descriptors_internal();
        manager
    }

    pub fn load_preset_descriptors(&mut self) -> Result<(), String> {
        self.load_preset_descriptors_internal()?;
        self.notify_changed();
        Ok(())
    }

    fn load_preset_descriptors_internal(&mut self) -> Result<(), String> {
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
        self.preset_descriptors = preset_file_paths
            .filter_map(|path| match Self::load_preset_data(path) {
                Ok((id, pd)) => Some(pd.into_descriptor(id)),
                Err(msg) => {
                    notification::warn(msg);
                    None
                }
            })
            .collect();
        self.preset_descriptors
            .sort_unstable_by_key(|p| p.name.to_lowercase());
        Ok(())
    }

    pub fn preset_descriptors(
        &self,
    ) -> impl Iterator<Item = &PresetDescriptor> + ExactSizeIterator {
        self.preset_descriptors.iter()
    }

    pub fn find_by_index(&self, index: usize) -> Option<&PresetDescriptor> {
        self.preset_descriptors.get(index)
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
        let _ = self.load_preset_descriptors();
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
            self.preset_descriptors.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    fn notify_changed(&mut self) {
        self.changed_subject.next(());
    }

    fn get_preset_file_path(&self, id: &str) -> PathBuf {
        self.preset_dir_path.join(format!("{}.json", id))
    }

    fn load_preset_data_internal(path: &Path) -> Result<PD, String> {
        let json = fs::read_to_string(path)
            .map_err(|_| format!("Couldn't read preset file \"{}\".", path.display()))?;
        serde_json::from_str(&json).map_err(|e| {
            format!(
                "Preset file {} isn't valid. Details:\n\n{}",
                path.display(),
                e
            )
        })
    }

    fn convert_to_model(id: &str, path: &Path, data: PD) -> Result<P, String> {
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
        data.to_model(id.to_owned())
    }

    fn load_preset_data(path: impl AsRef<Path>) -> Result<(String, PD), String> {
        let path = path.as_ref();
        let id = path
            .file_stem()
            .ok_or_else(|| {
                format!(
                    "Preset file \"{}\" only has an extension but not a name. \
                    The name is necessary because it makes up the preset ID.",
                    path.display()
                )
            })?
            .to_string_lossy()
            .to_string();
        let data = Self::load_preset_data_internal(path)?;
        Ok((id, data))
    }
}

impl<P: Preset, PD: PresetData<P = P>> ExtendedPresetManager for FileBasedPresetManager<P, PD> {
    fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.preset_descriptors.iter().position(|p| &p.id == id)
    }

    fn find_id_by_index(&self, index: usize) -> Option<String> {
        let preset = self.find_by_index(index)?;
        Some(preset.id.clone())
    }

    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str> {
        let path = self.get_preset_file_path(id);
        fs::remove_file(path).map_err(|_| "couldn't delete preset file")?;
        let _ = self.load_preset_descriptors();
        Ok(())
    }
}

impl<P: Preset, PD: PresetData<P = P>> PresetManager for FileBasedPresetManager<P, PD> {
    type PresetType = P;

    fn load_by_id(&self, id: &str) -> Option<P> {
        let path = self.get_preset_file_path(id);
        self.preset_descriptors
            .iter()
            .find(|desc| &desc.id == id)
            .and_then(|desc| {
                let data = Self::load_preset_data_internal(&path).ok()?;
                Self::convert_to_model(id, &path, data).ok()
            })
    }
}

pub trait PresetData: Sized + Serialize + DeserializeOwned + Debug {
    type P: Preset;

    fn from_model(preset: &Self::P) -> Self;

    fn to_model(&self, id: String) -> Result<Self::P, String>;

    fn clear_id(&mut self);

    fn version(&self) -> Option<&Version>;

    fn into_descriptor(self, id: String) -> PresetDescriptor;
}
