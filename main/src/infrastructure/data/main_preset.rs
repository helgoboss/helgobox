use crate::application::{
    MainPreset, ParameterSetting, Preset, PresetManager, SharedGroup, SharedMapping,
};
use crate::base::default_util::is_default;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::{
    CompartmentModelData, ExtendedPresetManager, FileBasedPresetManager, PresetData,
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
    #[serde(flatten)]
    data: CompartmentModelData,
}

impl PresetData for MainPresetData {
    type P = MainPreset;

    fn from_model(preset: &MainPreset) -> MainPresetData {
        MainPresetData {
            version: Some(App::version().clone()),
            id: Some(preset.id().to_string()),
            data: CompartmentModelData::from_model(preset.data()),
            name: preset.name().to_string(),
        }
    }

    fn to_model(&self, id: String) -> MainPreset {
        MainPreset::new(
            id,
            self.name.clone(),
            self.data
                .to_model(self.version.as_ref(), MappingCompartment::MainMappings),
        )
    }

    fn clear_id(&mut self) {
        self.id = None;
    }

    fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }
}
