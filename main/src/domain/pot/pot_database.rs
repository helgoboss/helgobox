use crate::base::{blocking_read_lock, blocking_write_lock};
use crate::domain::pot::provider_database::{Database, DatabaseId, InnerBuildOutput};
use crate::domain::pot::providers::komplete::KompleteDatabase;
use crate::domain::pot::providers::rfx_chain::RfxChainDatabase;
use crate::domain::pot::{
    BuildInput, FilterItem, FilterItemId, GenericBuildOutput, Preset, PresetId,
};

use indexmap::IndexSet;
use itertools::Itertools;
use realearn_api::persistence::PotFilterItemKind;
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
            .map(|(i, db)| (DatabaseId(i as _), RwLock::new(db)))
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
        let mut database_filter_items = Vec::new();
        // Let databases build collections
        let build_outputs: Vec<(DatabaseId, InnerBuildOutput)> = self
            .databases
            .iter()
            .filter_map(|(db_id, db)| {
                // Acquire database access
                let db = blocking_read_lock(db, "pot db build_collections");
                let db = db.as_ref().ok()?;
                // Create database filter item
                let filter_item = FilterItem {
                    persistent_id: "".to_string(),
                    id: FilterItemId(Some(db_id.0)),
                    parent_name: None,
                    name: Some(db.filter_item_name()),
                    icon: None,
                };
                database_filter_items.push(filter_item);
                // Don't build collections if database filter doesn't match
                if let Some(FilterItemId(Some(filter_db_id))) =
                    input.filter_settings.get(PotFilterItemKind::Database)
                {
                    if db_id.0 != filter_db_id {
                        return None;
                    }
                }
                // Let database build collections
                let output = db.build_collections(input.clone()).ok()?;
                Some((*db_id, output))
            })
            .collect();
        // Set database filter items
        total_output
            .filter_item_collections
            .set(PotFilterItemKind::Database, database_filter_items);
        // Process outputs
        total_output.preset_collection = build_outputs
            .into_iter()
            .flat_map(|(db_id, o)| {
                for (kind, items) in o.filter_item_collections.into_iter() {
                    total_output
                        .filter_item_collections
                        .extend(kind, items.into_iter());
                }
                // Merge stats
                total_output.stats.preset_query_duration += o.stats.preset_query_duration;
                total_output.stats.filter_query_duration += o.stats.filter_query_duration;
                // TODO-high Implement application of fixed filters. I guess we should actually
                //  fix filter settings in the caller of this function, not here!
                // total_output.filter_settings.
                o.preset_collection.into_iter().map(move |p| (db_id, p))
            })
            // Merge presets
            .sorted_by(|(_, p1), (_, p2)| p1.preset_name.cmp(&p2.preset_name))
            .map(|(db_id, p)| PresetId::new(db_id, p.inner_preset_id))
            .collect();
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

pub type BuildOutput = GenericBuildOutput<IndexSet<PresetId>>;
