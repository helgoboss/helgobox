use crate::application::{Controller, Preset, PresetManager, PrimaryPreset, SharedMapping};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{FileBasedPresetManager, MappingModelData, PresetData};

use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type FileBasedPrimaryPresetManager = FileBasedPresetManager<PrimaryPreset, PrimaryPresetData>;

pub type SharedPrimaryPresetManager = Rc<RefCell<FileBasedPrimaryPresetManager>>;

impl PresetManager for SharedPrimaryPresetManager {
    type PresetType = PrimaryPreset;

    fn find_by_id(&self, id: &str) -> Option<PrimaryPreset> {
        self.borrow().find_by_id(id)
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        self.borrow().mappings_are_dirty(id, mappings)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryPresetData {
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
}

impl PresetData for PrimaryPresetData {
    type P = PrimaryPreset;

    fn from_model(preset: &PrimaryPreset) -> PrimaryPresetData {
        PrimaryPresetData {
            id: Some(preset.id().to_string()),
            mappings: preset
                .mappings()
                .iter()
                .map(|m| MappingModelData::from_model(&m))
                .collect(),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> PrimaryPreset {
        PrimaryPreset::new(
            id,
            self.name.clone(),
            self.mappings
                .iter()
                .map(|m| m.to_model(MappingCompartment::PrimaryMappings, None))
                .collect(),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }
}
