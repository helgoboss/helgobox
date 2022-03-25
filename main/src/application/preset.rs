use crate::application::CompartmentModel;
use std::fmt;
use std::fmt::Debug;

pub trait Preset: Debug {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn data(&self) -> &CompartmentModel;
}

pub trait PresetManager: fmt::Debug {
    type PresetType;

    fn find_by_id(&self, id: &str) -> Option<Self::PresetType>;
}
