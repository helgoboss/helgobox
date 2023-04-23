use crate::domain::pot::{BuildInput, GenericBuildOutput, InnerPresetId, Preset};
use std::error::Error;
use std::path::PathBuf;

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
pub struct DatabaseId(pub u8);

impl DatabaseId {
    pub fn dummy() -> Self {
        Self(0)
    }
}

pub trait Database {
    fn refresh(&mut self) -> Result<(), Box<dyn Error>>;

    fn build_collections(&self, input: BuildInput) -> Result<InnerBuildOutput, Box<dyn Error>>;

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset>;

    fn find_preview_by_preset_id(&self, preset_id: InnerPresetId) -> Option<PathBuf>;
}

pub type InnerBuildOutput = GenericBuildOutput<Vec<SortablePresetId>>;

pub struct SortablePresetId {
    pub inner_preset_id: InnerPresetId,
    pub preset_name: String,
}

impl SortablePresetId {
    pub fn new(inner_preset_id: InnerPresetId, preset_name: String) -> Self {
        Self {
            inner_preset_id,
            preset_name,
        }
    }
}
