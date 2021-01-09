use crate::application::{GroupModel, MappingModel, Preset, SharedMapping};
use std::fmt;

#[derive(Clone, Debug)]
pub struct MainPreset {
    id: String,
    name: String,
    default_group: GroupModel,
    groups: Vec<GroupModel>,
    mappings: Vec<MappingModel>,
}

impl MainPreset {
    pub fn new(
        id: String,
        name: String,
        default_group: GroupModel,
        groups: Vec<GroupModel>,
        mappings: Vec<MappingModel>,
    ) -> MainPreset {
        MainPreset {
            id,
            name,
            default_group,
            groups,
            mappings,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update_data(
        &mut self,
        default_group: GroupModel,
        groups: Vec<GroupModel>,
        mappings: Vec<MappingModel>,
    ) {
        self.default_group = default_group;
        self.groups = groups;
        self.mappings = mappings;
    }
}

impl Preset for MainPreset {
    fn id(&self) -> &str {
        &self.id
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
}

impl fmt::Display for MainPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub trait MainPresetManager: fmt::Debug {
    fn find_by_id(&self, id: &str) -> Option<MainPreset>;

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool;
}
