use crate::application::{MainPreset, Preset, PresetManager, SharedMapping};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{
    ExtendedPresetManager, FileBasedPresetManager, MappingModelData, PresetData,
};

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

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        self.borrow().mappings_are_dirty(id, mappings)
    }
}

impl ExtendedPresetManager for SharedMainPresetManager {
    fn find_index_by_id(&self, id: &str) -> Option<usize> {
        self.borrow().find_index_by_id(id)
    }

    fn find_id_by_index(&self, index: usize) -> Option<String> {
        self.borrow().find_id_by_index(index)
    }

    fn remove_preset(&mut self, id: &str) -> Result<(), &'static str> {
        self.borrow_mut().remove_preset(id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainPresetData {
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
}

impl PresetData for MainPresetData {
    type P = MainPreset;

    fn from_model(preset: &MainPreset) -> MainPresetData {
        MainPresetData {
            id: Some(preset.id().to_string()),
            mappings: preset
                .mappings()
                .iter()
                .map(|m| MappingModelData::from_model(&m))
                .collect(),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> MainPreset {
        MainPreset::new(
            id,
            self.name.clone(),
            self.mappings
                .iter()
                .map(|m| m.to_model(MappingCompartment::MainMappings, None))
                .collect(),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }
}
