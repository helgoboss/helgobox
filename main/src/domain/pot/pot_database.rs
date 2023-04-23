use crate::base::{blocking_read_lock, blocking_write_lock};
use crate::domain::pot::provider_database::{Database, DatabaseId};
use crate::domain::pot::providers::komplete::KompleteDatabase;
use crate::domain::pot::providers::rfx_chain::RfxChainDatabase;
use crate::domain::pot::{BuildInput, BuildOutput, Preset, PresetId};

use reaper_high::Reaper;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::RwLock;

pub fn pot_db() -> &'static PotDatabase {
    use once_cell::sync::Lazy;
    static POT_DB: Lazy<PotDatabase> = Lazy::new(|| PotDatabase::open());
    &*POT_DB
}

type BoxedDatabase = Box<dyn Database + Send + Sync>;
type DatabaseOpeningResult = Result<BoxedDatabase, PotDatabaseError>;

pub struct PotDatabase {
    databases: BTreeMap<DatabaseId, RwLock<DatabaseOpeningResult>>,
}

#[derive(Clone, Debug, derive_more::Display)]
pub struct PotDatabaseError(String);

impl Error for PotDatabaseError {}

fn box_db<D: Database + Send + Sync + 'static>(
    opening_result: Result<D, Box<dyn Error>>,
) -> DatabaseOpeningResult {
    let db = opening_result.map_err(|e| PotDatabaseError(e.to_string()))?;
    Ok(Box::new(db))
}

impl PotDatabase {
    pub fn open() -> Self {
        let komplete_db = KompleteDatabase::open();
        let rfx_chain_db = RfxChainDatabase::open(Reaper::get().resource_path().join("FXChains"));
        let databases = [box_db(komplete_db), box_db(rfx_chain_db)];
        let databases = databases
            .into_iter()
            .enumerate()
            .map(|(i, db)| (DatabaseId(i as u8), RwLock::new(db)))
            .collect();
        let mut pot_database = Self { databases };
        pot_database.refresh();
        pot_database
    }

    pub fn refresh(&mut self) {
        for db in self.databases.values() {
            let mut db = blocking_write_lock(db, "pot db build_collections");
            let Some(db) = db.as_mut().ok() else {
                continue;
            };
            let _ = db.refresh();
        }
    }

    pub fn build_collections(&self, input: BuildInput) -> BuildOutput {
        let mut total_output = BuildOutput::default();
        let single_outputs = self.databases.iter().filter_map(|(db_id, db)| {
            let db = blocking_read_lock(db, "pot db build_collections");
            let db = db.as_ref().ok()?;
            let output = db.build_collections(input.clone()).ok()?;
            Some((db_id, output))
        });
        for (db_id, o) in single_outputs {
            for id in o.preset_collection.into_iter() {
                let qualified_preset_id = PresetId::new(*db_id, id);
                total_output.preset_collection.insert(qualified_preset_id);
            }
            for (kind, items) in o.filter_item_collections.into_iter() {
                total_output.filter_item_collections.set(kind, items);
            }
            total_output.stats.preset_query_duration += o.stats.preset_query_duration;
            total_output.stats.filter_query_duration += o.stats.filter_query_duration;
        }
        total_output
    }

    pub fn find_preset_by_id(&self, preset_id: PresetId) -> Option<Preset> {
        let db = self.databases.get(&preset_id.database_id)?;
        let db = blocking_read_lock(db, "pot db find_preset_by_id");
        let db = db.as_ref().ok()?;
        db.find_preset_by_id(preset_id.preset_id)
    }

    pub fn find_preview_file_by_preset_id(&self, preset_id: PresetId) -> Option<PathBuf> {
        let db = self.databases.get(&preset_id.database_id)?;
        let db = blocking_read_lock(db, "pot db find_preview_file_by_preset_id");
        let db = db.as_ref().ok()?;
        db.find_preview_by_preset_id(preset_id.preset_id)
    }
}
