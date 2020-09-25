use crate::application::MappingModel;
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

    pub fn mappings(&self) -> impl Iterator<Item = &MappingModel> {
        self.mappings.iter()
    }
}

impl fmt::Display for Controller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

pub trait ControllerManager: fmt::Debug {
    fn find_by_id(&self, id: &str) -> Option<Controller>;
}
