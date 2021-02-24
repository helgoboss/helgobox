use crate::application::{ControllerPreset, Preset, PresetManager, SharedGroup, SharedMapping};
use crate::core::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{
    ExtendedPresetManager, FileBasedPresetManager, MappingModelData, MigrationDescriptor,
    PresetData,
};

use crate::infrastructure::plugin::App;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type FileBasedControllerPresetManager =
    FileBasedPresetManager<ControllerPreset, ControllerPresetData>;

pub type SharedControllerPresetManager = Rc<RefCell<FileBasedControllerPresetManager>>;

impl PresetManager for SharedControllerPresetManager {
    type PresetType = ControllerPreset;

    fn find_by_id(&self, id: &str) -> Option<ControllerPreset> {
        self.borrow().find_by_id(id)
    }

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool {
        self.borrow().mappings_are_dirty(id, mappings)
    }

    fn groups_are_dirty(
        &self,
        id: &str,
        default_group: &SharedGroup,
        groups: &[SharedGroup],
    ) -> bool {
        self.borrow().groups_are_dirty(id, default_group, groups)
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerPresetData {
    // Since ReaLearn 1.12.0-pre18
    #[serde(default, skip_serializing_if = "is_default")]
    version: Option<Version>,
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    custom_data: HashMap<String, serde_json::Value>,
}

impl PresetData for ControllerPresetData {
    type P = ControllerPreset;

    fn from_model(controller: &ControllerPreset) -> ControllerPresetData {
        ControllerPresetData {
            version: Some(App::version().clone()),
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

    fn to_model(&self, id: String) -> ControllerPreset {
        let migration_descriptor = MigrationDescriptor::new(self.version.as_ref());
        ControllerPreset::new(
            id,
            self.name.clone(),
            self.mappings
                .iter()
                .map(|m| {
                    m.to_model(
                        MappingCompartment::ControllerMappings,
                        None,
                        &migration_descriptor,
                        self.version.as_ref(),
                    )
                })
                .collect(),
            self.custom_data.clone(),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }

    fn was_saved_with_newer_version(&self) -> bool {
        App::given_version_is_newer_than_app_version(self.version.as_ref())
    }
}
