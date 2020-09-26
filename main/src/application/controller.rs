use crate::application::{MappingModel, SharedMapping};
use rx_util::UnitEvent;
use std::fmt;

#[derive(Clone, Debug)]
pub struct Controller {
    id: String,
    name: String,
    mappings: Vec<MappingModel>,
}

impl Controller {
    pub fn new(id: String, name: String, mappings: Vec<MappingModel>) -> Controller {
        Controller { id, name, mappings }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn mappings(&self) -> impl Iterator<Item = &MappingModel> + ExactSizeIterator {
        self.mappings.iter()
    }

    pub fn update_mappings(&mut self, mappings: Vec<MappingModel>) {
        self.mappings = mappings;
    }
}

impl fmt::Display for Controller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub trait ControllerManager: fmt::Debug {
    fn find_by_id(&self, id: &str) -> Option<Controller>;

    fn mappings_are_dirty(&self, id: &str, mappings: &[SharedMapping]) -> bool;
}
