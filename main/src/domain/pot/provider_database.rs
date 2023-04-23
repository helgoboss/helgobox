use crate::domain::pot::{BuildInput, BuildOutput, Preset, PresetId};
use std::error::Error;
use std::path::PathBuf;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct DatabaseId(());

impl DatabaseId {
    pub fn dummy() -> Self {
        Self(())
    }
}

pub trait Database {
    fn id(&self) -> DatabaseId;

    fn build_collections(&self, input: BuildInput) -> Result<BuildOutput, Box<dyn Error>>;

    fn find_preset_by_id(&self, preset_id: PresetId) -> Option<Preset>;

    fn find_preview_by_preset_id(&self, preset_id: PresetId) -> Option<PathBuf>;
}
