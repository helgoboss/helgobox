use crate::domain::pot::{
    BuildInput, BuildOutput, InnerBuildOutput, InnerPresetId, Preset, PresetId,
};
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
