use crate::base::{blocking_read_lock, blocking_write_lock};
use crate::domain::pot::provider_database::{
    Database, DatabaseId, ProviderContext, FIL_CONTENT_TYPE_FACTORY, FIL_CONTENT_TYPE_USER,
    FIL_FAVORITE_FAVORITE, FIL_FAVORITE_NOT_FAVORITE, FIL_PRODUCT_KIND_EFFECT,
    FIL_PRODUCT_KIND_INSTRUMENT, FIL_PRODUCT_KIND_LOOP, FIL_PRODUCT_KIND_ONE_SHOT,
};
use crate::domain::pot::providers::directory::{DirectoryDatabase, DirectoryDbConfig};
use crate::domain::pot::providers::komplete::KompleteDatabase;
use crate::domain::pot::{
    BuildInput, Fil, FilterItem, FilterItemCollections, FilterItemId, Preset, PresetId, Stats,
};

use crate::domain::pot::plugins::PluginDatabase;
use crate::domain::pot::providers::defaults::DefaultsDatabase;
use crate::domain::pot::providers::ini::IniDatabase;
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::Reaper;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

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
        let resource_path = Reaper::get().resource_path();
        let komplete_db = KompleteDatabase::open();
        let rfx_chain_db = {
            let config = DirectoryDbConfig {
                root_dir: resource_path.join("FXChains"),
                valid_extensions: &["RfxChain"],
                name: "FX chains",
                publish_relative_path: true,
            };
            DirectoryDatabase::open(config)
        };
        let track_template_db = {
            let config = DirectoryDbConfig {
                root_dir: resource_path.join("TrackTemplates"),
                valid_extensions: &["RTrackTemplate"],
                name: "Track templates",
                publish_relative_path: false,
            };
            DirectoryDatabase::open(config)
        };
        let ini_db = IniDatabase::open(resource_path.join("presets"));
        let defaults_db = DefaultsDatabase::open();
        let databases = [
            box_db(komplete_db),
            box_db(rfx_chain_db),
            box_db(track_template_db),
            box_db(ini_db),
            box_db(Ok(defaults_db)),
        ];
        let databases = databases
            .into_iter()
            .enumerate()
            .map(|(i, db)| (DatabaseId(i as _), RwLock::new(db)))
            .collect();
        let pot_database = Self { databases };
        pot_database.refresh();
        pot_database
    }

    pub fn refresh(&self) {
        // Build provider context
        let resource_path = Reaper::get().resource_path();
        let plugin_db = PluginDatabase::crawl(&resource_path);
        let provider_context = ProviderContext {
            plugin_db: &plugin_db,
        };
        // Refresh databases
        for db in self.databases.values() {
            let mut db = blocking_write_lock(db, "pot db build_collections");
            let Some(db) = db.as_mut().ok() else {
                continue;
            };
            let _ = db.refresh(&provider_context);
        }
    }

    pub fn build_collections(&self, mut input: BuildInput) -> BuildOutput {
        // Build constant filter collections
        let mut total_output = BuildOutput::default();
        measure_duration(&mut total_output.stats.filter_query_duration, || {
            if input.affected_kinds.contains(PotFilterItemKind::IsUser) {
                total_output.filter_item_collections.set(
                    PotFilterItemKind::IsUser,
                    vec![
                        FilterItem::simple(FIL_CONTENT_TYPE_USER, "User", '🕵'),
                        FilterItem::simple(FIL_CONTENT_TYPE_FACTORY, "Factory", '🏭'),
                    ],
                );
            }
            if input
                .affected_kinds
                .contains(PotFilterItemKind::ProductKind)
            {
                total_output.filter_item_collections.set(
                    PotFilterItemKind::ProductKind,
                    vec![
                        FilterItem::none(),
                        FilterItem::simple(FIL_PRODUCT_KIND_INSTRUMENT, "Instrument", '🎹'),
                        FilterItem::simple(FIL_PRODUCT_KIND_EFFECT, "Effect", '✨'),
                        FilterItem::simple(FIL_PRODUCT_KIND_LOOP, "Loop", '➿'),
                        FilterItem::simple(FIL_PRODUCT_KIND_ONE_SHOT, "One shot", '💥'),
                    ],
                );
            }
            if input.affected_kinds.contains(PotFilterItemKind::IsFavorite) {
                total_output.filter_item_collections.set(
                    PotFilterItemKind::IsFavorite,
                    vec![
                        FilterItem::simple(FIL_FAVORITE_FAVORITE, "Favorite", '★'),
                        FilterItem::simple(FIL_FAVORITE_NOT_FAVORITE, "Not favorite", '☆'),
                    ],
                );
            }
            // Let all databases build filter collections and accumulate them
            let mut database_filter_items = Vec::new();
            for (db_id, db) in &self.databases {
                // If the database is on the exclude list, we don't even want it to appear in the
                // database list.
                if input.filter_exclude_list.excludes_database(*db_id) {
                    continue;
                }
                // Acquire database access
                let db = blocking_read_lock(db, "pot db build_collections 1");
                let Ok(db) = db.as_ref() else {
                    continue
                };
                // Create database filter item
                let filter_item = FilterItem {
                    persistent_id: "".to_string(),
                    id: FilterItemId(Some(Fil::Database(*db_id))),
                    parent_name: None,
                    name: Some(db.filter_item_name()),
                    icon: None,
                };
                database_filter_items.push(filter_item);
                // Don't continue if database doesn't match filter
                // (but it should appear on the list)
                if !input.filter_settings.database_matches(*db_id) {
                    continue;
                }
                // Build and accumulate filters collections
                let Ok(filter_collections) = db.query_filter_collections(&input) else {
                    continue;
                };
                for (kind, items) in filter_collections.into_iter() {
                    total_output
                        .filter_item_collections
                        .extend(kind, items.into_iter());
                }
            }
            // Add database filter items
            if input.affected_kinds.contains(PotFilterItemKind::Database) {
                total_output
                    .filter_item_collections
                    .set(PotFilterItemKind::Database, database_filter_items);
            }
            // Important: At this point, some previously selected filters might not exist anymore.
            // So we should reset them and not let them influence the preset query anymore!
            input.filter_settings.clear_if_not_available_anymore(
                input.affected_kinds,
                &total_output.filter_item_collections,
            );
        });
        // Finally build and accumulate presets
        let mut sortable_preset_ids: Vec<_> =
            measure_duration(&mut total_output.stats.preset_query_duration, || {
                self.databases
                    .iter()
                    .filter(|(db_id, _)| {
                        input.filter_settings.database_matches(**db_id)
                            && !input.filter_exclude_list.excludes_database(**db_id)
                    })
                    .filter_map(|(db_id, db)| {
                        // Acquire database access
                        let db = blocking_read_lock(db, "pot db build_collections 2");
                        let db = db.as_ref().ok()?;
                        // Let database build presets
                        let preset_ids = db.query_presets(&input).ok()?;
                        Some((*db_id, preset_ids))
                    })
                    .flat_map(|(db_id, preset_ids)| preset_ids.into_iter().map(move |p| (db_id, p)))
                    .collect()
            });
        // Sort presets
        measure_duration(&mut total_output.stats.sort_duration, || {
            sortable_preset_ids.sort_by(|(_, p1), (_, p2)| {
                lexical_sort::lexical_cmp(&p1.preset_name, &p2.preset_name)
                // lexical_sort::cmp(&p1.preset_name, &p2.preset_name)
            });
        });
        // Index presets
        measure_duration(&mut total_output.stats.index_duration, || {
            total_output.preset_collection = sortable_preset_ids
                .into_iter()
                .map(|(db_id, p)| PresetId::new(db_id, p.inner_preset_id))
                .collect();
        });
        total_output
    }

    pub fn find_preset_by_id(&self, preset_id: PresetId) -> Option<Preset> {
        let db = self.databases.get(&preset_id.database_id)?;
        let db = blocking_read_lock(db, "pot db find_preset_by_id");
        let db = db.as_ref().ok()?;
        db.find_preset_by_id(preset_id.preset_id)
    }

    pub fn try_find_preset_by_id(
        &self,
        preset_id: PresetId,
    ) -> Result<Option<Preset>, &'static str> {
        let db = self
            .databases
            .get(&preset_id.database_id)
            .ok_or("database not found")?;
        let db = db.try_read().map_err(|_| "couldn't acquire lock")?;
        let db = db.as_ref().map_err(|_| "database not opened")?;
        Ok(db.find_preset_by_id(preset_id.preset_id))
    }

    pub fn find_preview_file_by_preset_id(&self, preset_id: PresetId) -> Option<PathBuf> {
        let db = self.databases.get(&preset_id.database_id)?;
        let db = blocking_read_lock(db, "pot db find_preview_file_by_preset_id");
        let db = db.as_ref().ok()?;
        db.find_preview_by_preset_id(preset_id.preset_id)
    }
}

#[derive(Default)]
pub struct BuildOutput {
    pub filter_item_collections: FilterItemCollections,
    pub preset_collection: IndexSet<PresetId>,
    pub stats: Stats,
}

fn measure_duration<R>(duration: &mut Duration, f: impl FnOnce() -> R) -> R {
    let start = Instant::now();
    let r = f();
    *duration = start.elapsed();
    r
}
