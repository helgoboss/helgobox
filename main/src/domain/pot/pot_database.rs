use crate::base::blocking_lock;
use crate::domain::pot::api::QualifiedPresetId;
use crate::domain::pot::provider_database::{Database, DatabaseId};
use crate::domain::pot::providers::komplete::KompleteDatabase;
use crate::domain::pot::{BuildInput, BuildOutput, Preset, PresetId};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::path::PathBuf;
use std::sync::Mutex;

pub fn pot_db() -> &'static PotDatabase {
    use once_cell::sync::Lazy;
    static POT_DB: Lazy<PotDatabase> = Lazy::new(|| PotDatabase::open());
    &*POT_DB
}

pub struct PotDatabase {
    komplete: Result<KompleteDatabase, PotDatabaseError>,
}

#[derive(Debug, derive_more::Display)]
struct PotDatabaseError(String);

impl Error for PotDatabaseError {}

impl PotDatabase {
    pub fn open() -> Self {
        Self {
            komplete: KompleteDatabase::open().map_err(|e| PotDatabaseError(e.to_string())),
        }
    }

    pub fn build_collections(&self, input: BuildInput) -> Result<BuildOutput, Box<dyn Error + '_>> {
        let komplete = self.komplete.as_ref()?;
        komplete.build_collections(input)
    }

    pub fn find_legacy_preset_by_id(&self, preset_id: PresetId) -> Option<Preset> {
        self.find_preset_by_id(QualifiedPresetId::new(DatabaseId::dummy(), preset_id))
    }

    pub fn find_preset_by_id(&self, preset_id: QualifiedPresetId) -> Option<Preset> {
        let komplete = self.komplete.as_ref().ok()?;
        komplete.find_preset_by_id(preset_id.preset_id)
    }

    pub fn find_legacy_preview_file_by_preset_id(&self, preset_id: PresetId) -> Option<PathBuf> {
        self.find_preview_file_by_preset_id(QualifiedPresetId::new(DatabaseId::dummy(), preset_id))
    }

    pub fn find_preview_file_by_preset_id(&self, preset_id: QualifiedPresetId) -> Option<PathBuf> {
        let komplete = self.komplete.as_ref().ok()?;
        komplete.find_preview_by_preset_id(preset_id.preset_id)
    }
}
