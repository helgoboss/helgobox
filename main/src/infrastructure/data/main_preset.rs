use crate::application::{MainPreset, Preset, PresetManager};
use crate::domain::Compartment;
use crate::infrastructure::data::{CompartmentModelData, FileBasedPresetManager, PresetData};
use base::default_util::{deserialize_null_default, is_default};

use crate::infrastructure::plugin::BackboneShell;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

pub type FileBasedMainPresetManager = FileBasedPresetManager<MainPreset, MainPresetData>;

pub type SharedMainPresetManager = Rc<RefCell<FileBasedMainPresetManager>>;

impl PresetManager for SharedMainPresetManager {
    type PresetType = MainPreset;

    fn find_by_id(&self, id: &str) -> Option<MainPreset> {
        self.borrow().find_by_id(id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPresetData {
    // Since ReaLearn 1.12.0-pre18
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    version: Option<Version>,
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(flatten)]
    data: CompartmentModelData,
}

impl PresetData for MainPresetData {
    type P = MainPreset;

    fn from_model(preset: &MainPreset) -> MainPresetData {
        MainPresetData {
            version: Some(BackboneShell::version().clone()),
            id: Some(preset.id().to_string()),
            data: CompartmentModelData::from_model(preset.data()),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> anyhow::Result<MainPreset> {
        let preset = MainPreset::new(
            id,
            self.name.clone(),
            self.data
                .to_model(self.version.as_ref(), Compartment::Main, None)?,
        );
        Ok(preset)
    }

    fn clear_id(&mut self) {
        self.id = None;
    }

    fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}
