use crate::application::{MappingModel, Preset};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug)]
pub struct ControllerPreset {
    id: String,
    name: String,
    mappings: Vec<MappingModel>,
    custom_data: HashMap<String, serde_json::Value>,
}

impl ControllerPreset {
    pub fn new(
        id: String,
        name: String,
        mappings: Vec<MappingModel>,
        custom_data: HashMap<String, serde_json::Value>,
    ) -> ControllerPreset {
        ControllerPreset {
            id,
            name,
            mappings,
            custom_data,
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn custom_data(&self) -> &HashMap<String, serde_json::Value> {
        &self.custom_data
    }

    pub fn update_custom_data(&mut self, key: String, value: serde_json::Value) {
        self.custom_data.insert(key, value);
    }

    pub fn update_mappings(&mut self, mappings: Vec<MappingModel>) {
        self.mappings = mappings;
    }
}

impl Preset for ControllerPreset {
    fn id(&self) -> &str {
        &self.id
    }

    fn mappings(&self) -> &Vec<MappingModel> {
        &self.mappings
    }
}

impl fmt::Display for ControllerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
