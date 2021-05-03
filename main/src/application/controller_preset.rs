use crate::application::{GroupModel, MappingModel, ParameterSetting, Preset};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug)]
pub struct ControllerPreset {
    id: String,
    name: String,
    default_group: GroupModel,
    groups: Vec<GroupModel>,
    mappings: Vec<MappingModel>,
    parameters: HashMap<u32, ParameterSetting>,
    custom_data: HashMap<String, serde_json::Value>,
}

impl ControllerPreset {
    pub fn new(
        id: String,
        name: String,
        default_group: GroupModel,
        groups: Vec<GroupModel>,
        mappings: Vec<MappingModel>,
        parameters: HashMap<u32, ParameterSetting>,
        custom_data: HashMap<String, serde_json::Value>,
    ) -> ControllerPreset {
        ControllerPreset {
            id,
            name,
            default_group,
            groups,
            mappings,
            parameters,
            custom_data,
        }
    }

    pub fn custom_data(&self) -> &HashMap<String, serde_json::Value> {
        &self.custom_data
    }

    pub fn update_custom_data(&mut self, key: String, value: serde_json::Value) {
        self.custom_data.insert(key, value);
    }

    pub fn update_realearn_data(
        &mut self,
        default_group: GroupModel,
        groups: Vec<GroupModel>,
        mappings: Vec<MappingModel>,
        parameters: HashMap<u32, ParameterSetting>,
    ) {
        self.default_group = default_group;
        self.groups = groups;
        self.mappings = mappings;
        self.parameters = parameters;
    }
}

impl Preset for ControllerPreset {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn default_group(&self) -> &GroupModel {
        &self.default_group
    }

    fn groups(&self) -> &Vec<GroupModel> {
        &self.groups
    }

    fn mappings(&self) -> &Vec<MappingModel> {
        &self.mappings
    }

    fn parameters(&self) -> &HashMap<u32, ParameterSetting> {
        &self.parameters
    }
}

impl fmt::Display for ControllerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
