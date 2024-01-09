use crate::application::CompartmentModel;
use crate::domain::Compartment;
use std::fmt;

pub trait CompartmentPresetManager: fmt::Debug {
    fn find_by_id(&self, id: &str) -> Option<CompartmentPresetModel>;
}

#[derive(Clone, Debug)]
pub struct CompartmentPresetModel {
    id: String,
    name: String,
    compartment: Compartment,
    model: CompartmentModel,
}

impl CompartmentPresetModel {
    pub fn new(
        id: String,
        name: String,
        compartment: Compartment,
        model: CompartmentModel,
    ) -> CompartmentPresetModel {
        CompartmentPresetModel {
            id,
            name,
            compartment,
            model,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn compartment(&self) -> Compartment {
        self.compartment
    }

    pub fn model(&self) -> &CompartmentModel {
        &self.model
    }

    pub fn set_model(&mut self, data: CompartmentModel) {
        self.model = data;
    }

    pub fn patch_custom_data(&mut self, key: String, value: serde_json::Value) {
        self.model.custom_data.insert(key, value);
    }
}

impl fmt::Display for CompartmentPresetModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
