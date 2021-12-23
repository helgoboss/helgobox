use crate::application::CompartmentModel;
use std::fmt;
use std::fmt::Debug;

pub trait Preset: Clone + Debug {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn data(&self) -> &CompartmentModel;
}

pub trait PresetManager: fmt::Debug {
    type PresetType;

    // TODO-high Turn into Result<Self::PresetType, String> and expose load error
    fn load_by_id(&self, id: &str) -> Option<Self::PresetType>;
}
