use crate::application::{ControllerPreset, Preset, PresetManager};
use crate::domain::Compartment;
use crate::infrastructure::data::{
    CompartmentModelData, ExtendedPresetManager, FileBasedPresetManager, PresetData, PresetInfo,
};

use crate::base::default_util::{deserialize_null_default, is_default};
use crate::infrastructure::plugin::App;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

pub type FileBasedControllerPresetManager =
    FileBasedPresetManager<ControllerPreset, ControllerPresetData>;

pub type SharedControllerPresetManager = Rc<RefCell<FileBasedControllerPresetManager>>;

impl PresetManager for SharedControllerPresetManager {
    type PresetType = ControllerPreset;

    fn find_by_id(&self, id: &str) -> Option<ControllerPreset> {
        self.borrow().find_by_id(id)
    }
}

impl ExtendedPresetManager for SharedControllerPresetManager {
    fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.borrow().find_index_by_id(id)
    }

    fn find_id_by_index(&self, index: usize) -> Option<String> {
        self.borrow().find_id_by_index(index)
    }

    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str> {
        self.borrow_mut().remove_preset(id)
    }

    fn preset_infos(&self) -> Vec<PresetInfo> {
        self.borrow().preset_infos()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerPresetData {
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

impl PresetData for ControllerPresetData {
    type P = ControllerPreset;

    fn from_model(preset: &ControllerPreset) -> ControllerPresetData {
        ControllerPresetData {
            version: Some(App::version().clone()),
            id: Some(preset.id().to_string()),
            data: CompartmentModelData::from_model(preset.data()),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> Result<ControllerPreset, String> {
        let preset = ControllerPreset::new(
            id,
            self.name.clone(),
            self.data
                .to_model(self.version.as_ref(), Compartment::Controller, None)?,
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
