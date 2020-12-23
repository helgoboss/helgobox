use crate::application::{Controller, Preset, PresetManager, SharedMapping};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{
    ExtendedPresetManager, FileBasedPresetManager, MappingModelData, PresetData,
};

use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type FileBasedControllerManager = FileBasedPresetManager<Controller, ControllerData>;

pub type SharedControllerManager = Rc<RefCell<FileBasedControllerManager>>;

impl PresetManager for SharedControllerManager {
    type PresetType = Controller;

    fn find_by_id(&self, id: &str) -> Option<Controller> {
        self.borrow().find_by_id(id)
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        self.borrow().mappings_are_dirty(id, mappings)
    }
}

impl ExtendedPresetManager for SharedControllerManager {
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
pub struct ControllerData {
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    custom_data: HashMap<String, serde_json::Value>,
}

impl PresetData for ControllerData {
    type P = Controller;

    fn from_model(controller: &Controller) -> ControllerData {
        ControllerData {
            id: Some(controller.id().to_string()),
            mappings: controller
                .mappings()
                .iter()
                .map(|m| MappingModelData::from_model(&m))
                .collect(),
            name: controller.name().to_string(),
            custom_data: controller.custom_data().clone(),
        }
    }

    fn to_model(&self, id: String) -> Controller {
        Controller::new(
            id,
            self.name.clone(),
            self.mappings
                .iter()
                .map(|m| m.to_model(MappingCompartment::ControllerMappings, None))
                .collect(),
            self.custom_data.clone(),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }
}
