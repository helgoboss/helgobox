//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use enum_map::EnumMap;
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Mutex;

pub mod nks;

pub type FilterItemId = nks::FilterItemId;
pub type PresetId = nks::PresetId;
pub type PresetDb = nks::PresetDb;

pub fn with_preset_db<R>(f: impl FnOnce(&PresetDb) -> R) -> Result<R, &'static str> {
    nks::with_preset_db(f)
}

pub fn preset_db() -> Result<&'static Mutex<PresetDb>, &'static str> {
    nks::preset_db()
}

#[derive(Debug, Default)]
pub struct NavigationState {
    state: PersistentNavigationState,
    collections: Collections,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentNavigationState {
    filter_item_ids: FilterItemIds,
    preset_id: Option<PresetId>,
}

type FilterItemIds = EnumMap<PotFilterItemKind, Option<FilterItemId>>;
type PresetCollection = IndexSet<PresetId>;
type FilterItemCollections = EnumMap<PotFilterItemKind, Vec<FilterItem>>;

#[derive(Debug, Default)]
pub struct Collections {
    filter_item_collections: FilterItemCollections,
    preset_collection: PresetCollection,
}

#[derive(Debug, Default)]
pub struct CurrentPreset {
    param_mapping: HashMap<u32, u32>,
}

impl CurrentPreset {
    pub fn find_mapped_parameter_index_at(&self, slot_index: u32) -> Option<u32> {
        self.param_mapping.get(&slot_index).copied()
    }
}

impl NavigationState {
    pub fn preset_id(&self) -> Option<PresetId> {
        self.state.preset_id
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.state.preset_id = id;
    }

    pub fn filter_item_id(&self, kind: PotFilterItemKind) -> Option<FilterItemId> {
        self.state.filter_item_ids[kind]
    }

    pub fn set_filter_item_id(&mut self, kind: PotFilterItemKind, id: Option<FilterItemId>) {
        self.state.filter_item_ids[kind] = id;
    }

    pub fn rebuild_indexes(&mut self) -> Result<(), Box<dyn Error>> {
        let (state, collections) = with_preset_db(|db| db.build_collections(&self.state))??;
        self.state = state;
        self.collections = collections;
        Ok(())
    }

    pub fn count_filter_items(&self, kind: PotFilterItemKind) -> u32 {
        self.collections.filter_item_collections[kind].len() as _
    }

    pub fn count_presets(&self) -> u32 {
        self.collections.preset_collection.len() as u32
    }

    pub fn find_index_of_preset(&self, id: PresetId) -> Option<u32> {
        let index = self.collections.preset_collection.get_index_of(&id)?;
        Some(index as _)
    }

    pub fn find_preset_id_at_index(&self, index: u32) -> Option<PresetId> {
        self.collections
            .preset_collection
            .get_index(index as _)
            .copied()
    }

    pub fn find_filter_item_id_at_index(
        &self,
        kind: PotFilterItemKind,
        index: u32,
    ) -> Option<FilterItemId> {
        let item = self.collections.filter_item_collections[kind].get(index as usize)?;
        Some(item.id)
    }

    pub fn find_index_of_filter_item(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<u32> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.0)
    }

    pub fn find_filter_item_by_id(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<&FilterItem> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.1)
    }

    fn find_filter_item_and_index_by_id(
        &self,
        kind: PotFilterItemKind,
        id: FilterItemId,
    ) -> Option<(u32, &FilterItem)> {
        let (i, item) = self.collections.filter_item_collections[kind]
            .iter()
            .enumerate()
            .find(|(_, item)| item.id == id)?;
        Some((i as u32, item))
    }
}

#[derive(Clone, Debug)]
pub struct FilterItem {
    pub id: FilterItemId,
    pub name: String,
}

#[derive(Debug)]
pub struct Preset {
    pub id: PresetId,
    pub name: String,
    pub file_name: PathBuf,
    pub file_ext: String,
}

#[derive(serde::Deserialize)]
struct ParamAssignment {
    id: Option<u32>,
}
