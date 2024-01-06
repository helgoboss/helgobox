use crate::application::CompartmentModel;
use crate::domain::Compartment;
use std::fmt;
use std::fmt::Debug;

pub trait Preset: Debug {
    fn from_parts(id: String, name: String, compartment_model: CompartmentModel) -> Self;
    fn compartment() -> Compartment;
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn data(&self) -> &CompartmentModel;
}

pub trait PresetManager: fmt::Debug {
    type PresetType;

    fn find_by_id(&self, id: &str) -> Option<Self::PresetType>;
}
