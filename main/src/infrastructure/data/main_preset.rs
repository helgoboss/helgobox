use crate::application::{
    GroupModel, MainPreset, ParameterSetting, Preset, PresetManager, SharedGroup, SharedMapping,
};
use crate::base::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{
    ExtendedPresetManager, FileBasedPresetManager, GroupModelData, MappingModelData,
    MigrationDescriptor, PresetData,
};

use crate::infrastructure::plugin::App;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
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

    fn parameter_settings_are_dirty(
        &self,
        id: &str,
        parameter_settings: &HashMap<u32, ParameterSetting>,
    ) -> bool {
        self.borrow()
            .parameter_settings_are_dirty(id, parameter_settings)
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
    // Since ReaLearn 1.12.0-pre18
    #[serde(default, skip_serializing_if = "is_default")]
    version: Option<Version>,
    #[serde(skip_deserializing, skip_serializing_if = "is_default")]
    id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    default_group: Option<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    groups: Vec<GroupModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    mappings: Vec<MappingModelData>,
    #[serde(default, skip_serializing_if = "is_default")]
    parameters: HashMap<u32, ParameterSetting>,
}

impl PresetData for MainPresetData {
    type P = MainPreset;

    fn from_model(preset: &MainPreset) -> MainPresetData {
        MainPresetData {
            version: Some(App::version().clone()),
            id: Some(preset.id().to_string()),
            default_group: Some(GroupModelData::from_model(preset.default_group())),
            groups: preset
                .groups()
                .iter()
                .map(|g| GroupModelData::from_model(g))
                .collect(),
            mappings: preset
                .mappings()
                .iter()
                .map(|m| MappingModelData::from_model(m))
                .collect(),
            parameters: preset.parameters().clone(),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> MainPreset {
        let compartment = MappingCompartment::MainMappings;
        let migration_descriptor = MigrationDescriptor::new(self.version.as_ref());
        let final_default_group = self
            .default_group
            .as_ref()
            .map(|g| g.to_model(compartment))
            .unwrap_or_else(|| GroupModel::default_for_compartment(compartment));
        MainPreset::new(
            id,
            self.name.clone(),
            final_default_group,
            self.groups
                .iter()
                .map(|g| g.to_model(compartment))
                .collect(),
            self.mappings
                .iter()
                .map(|m| {
                    m.to_model_flexible(
                        compartment,
                        None,
                        &migration_descriptor,
                        self.version.as_ref(),
                    )
                })
                .collect(),
            self.parameters.clone(),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }

    fn was_saved_with_newer_version(&self) -> bool {
        App::given_version_is_newer_than_app_version(self.version.as_ref())
    }
}
