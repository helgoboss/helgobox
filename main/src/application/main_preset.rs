use crate::application::{CompartmentModel, Preset, SharedMapping};
use std::fmt;

#[derive(Clone, Debug)]
pub struct MainPreset {
    id: String,
    name: String,
    data: CompartmentModel,
}

impl MainPreset {
    pub fn new(id: String, name: String, data: CompartmentModel) -> MainPreset {
        MainPreset { id, name, data }
    }

    pub fn update_data(&mut self, data: CompartmentModel) {
        self.data = data;
    }
}

impl Preset for MainPreset {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn data(&self) -> &CompartmentModel {
        &self.data
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
