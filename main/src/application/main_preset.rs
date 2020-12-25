use crate::application::{MappingModel, Preset, SharedMapping};
use std::fmt;

#[derive(Clone, Debug)]
pub struct MainPreset {
    id: String,
    name: String,
    mappings: Vec<MappingModel>,
}

impl MainPreset {
    pub fn new(id: String, name: String, mappings: Vec<MappingModel>) -> MainPreset {
        MainPreset { id, name, mappings }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update_mappings(&mut self, mappings: Vec<MappingModel>) {
        self.mappings = mappings;
    }
}

impl Preset for MainPreset {
    fn id(&self) -> &str {
        &self.id
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
