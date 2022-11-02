//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use crate::domain::pot::nks::{NksFilterSettings, PersistentNksFilterSettings};
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

#[derive(Debug)]
pub enum PotUnit {
    Unloaded {
        state: PersistentState,
        previous_load_error: &'static str,
    },
    Loaded(RuntimePotUnit),
}

impl Default for PotUnit {
    fn default() -> Self {
        Self::unloaded(Default::default())
    }
}

impl PotUnit {
    pub fn unloaded(state: PersistentState) -> Self {
        Self::Unloaded {
            state,
            previous_load_error: "",
        }
    }

    pub fn loaded(&mut self) -> Result<&mut RuntimePotUnit, &'static str> {
        match self {
            PotUnit::Unloaded {
                state,
                previous_load_error,
            } => {
                if !previous_load_error.is_empty() {
                    return Err(previous_load_error);
                }
                match RuntimePotUnit::load(state) {
                    Ok(u) => {
                        *self = Self::Loaded(u);
                        self.loaded()
                    }
                    Err(e) => {
                        *previous_load_error = e;
                        Err(e)
                    }
                }
            }
            PotUnit::Loaded(p) => Ok(p),
        }
    }

    pub fn persistent_state(&self) -> PersistentState {
        match self {
            PotUnit::Unloaded { state, .. } => state.clone(),
            PotUnit::Loaded(u) => u.persistent_state(),
        }
    }
}

#[derive(Debug)]
pub struct RuntimePotUnit {
    runtime_state: RuntimeState,
    collections: Collections,
}

#[derive(Debug, Default)]
pub struct RuntimeState {
    filter_settings: FilterSettings,
    preset_id: Option<PresetId>,
}

impl RuntimeState {
    pub fn load(persistent_state: &PersistentState) -> Result<Self, &'static str> {
        with_preset_db(|db| {
            let filter_settings = db
                .build_filter_items(Default::default())
                .map(|(_, collections)| {
                    let find_id = |setting: &Option<String>, items: &[FilterItem]| {
                        setting.as_ref().and_then(|persistent_id| {
                            let item = items
                                .iter()
                                .find(|item| &item.persistent_id == persistent_id)?;
                            Some(item.id)
                        })
                    };
                    let nks = &persistent_state.filter_settings.nks;
                    FilterSettings {
                        nks: NksFilterSettings {
                            bank: find_id(&nks.bank, &collections.banks),
                            sub_bank: find_id(&nks.sub_bank, &collections.sub_banks),
                            category: find_id(&nks.category, &collections.categories),
                            sub_category: find_id(&nks.sub_category, &collections.sub_categories),
                            mode: find_id(&nks.mode, &collections.modes),
                        },
                    }
                })
                .unwrap_or_default();
            let preset_id = persistent_state
                .preset_id
                .as_ref()
                .and_then(|persistent_id| db.find_preset_id_by_favorite_id(persistent_id));
            Self {
                filter_settings,
                preset_id,
            }
        })
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentState {
    filter_settings: PersistentFilterSettings,
    preset_id: Option<String>,
}

type PresetCollection = IndexSet<PresetId>;

#[derive(Debug, Default)]
pub struct FilterItemCollections {
    pub databases: Vec<FilterItem>,
    pub nks: nks::FilterNksItemCollections,
}

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentFilterSettings {
    pub nks: nks::PersistentNksFilterSettings,
}

#[derive(Debug, Default)]
pub struct FilterSettings {
    pub nks: nks::NksFilterSettings,
}

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

impl RuntimePotUnit {
    pub fn load(state: &PersistentState) -> Result<Self, &'static str> {
        let mut unit = Self {
            runtime_state: RuntimeState::load(state)?,
            collections: Default::default(),
        };
        unit.rebuild_collections()
            .map_err(|_| "couldn't rebuild collections on load")?;
        Ok(unit)
    }

    pub fn persistent_state(&self) -> PersistentState {
        let find_id = |setting: Option<FilterItemId>, items: &[FilterItem]| {
            setting.and_then(|id| {
                items
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.persistent_id.clone())
            })
        };
        let nks_settings = &self.runtime_state.filter_settings.nks;
        let nks_items = &self.collections.filter_item_collections.nks;
        let filter_settings = PersistentFilterSettings {
            nks: PersistentNksFilterSettings {
                bank: find_id(nks_settings.bank, &nks_items.banks),
                sub_bank: find_id(nks_settings.sub_bank, &nks_items.sub_banks),
                category: find_id(nks_settings.category, &nks_items.categories),
                sub_category: find_id(nks_settings.sub_category, &nks_items.sub_categories),
                mode: find_id(nks_settings.mode, &nks_items.modes),
            },
        };
        let preset_id = self.runtime_state.preset_id.and_then(|id| {
            with_preset_db(|db| Some(db.find_preset_by_id(id)?.favorite_id))
                .ok()
                .flatten()
        });
        PersistentState {
            filter_settings,
            preset_id,
        }
    }

    pub fn state(&self) -> &RuntimeState {
        &self.runtime_state
    }

    pub fn preset_id(&self) -> Option<PresetId> {
        self.runtime_state.preset_id
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.runtime_state.preset_id = id;
    }

    pub fn filter_item_id(&self, kind: PotFilterItemKind) -> Option<FilterItemId> {
        use PotFilterItemKind::*;
        let settings = &self.runtime_state.filter_settings.nks;
        match kind {
            Database => None,
            NksBank => settings.bank,
            NksSubBank => settings.sub_bank,
            NksCategory => settings.category,
            NksSubCategory => settings.sub_category,
            NksMode => settings.mode,
        }
    }

    pub fn set_filter_item_id(&mut self, kind: PotFilterItemKind, id: Option<FilterItemId>) {
        use PotFilterItemKind::*;
        let settings = &mut self.runtime_state.filter_settings.nks;
        let prop = match kind {
            NksBank => &mut settings.bank,
            NksSubBank => &mut settings.sub_bank,
            NksCategory => &mut settings.category,
            NksSubCategory => &mut settings.sub_category,
            NksMode => &mut settings.mode,
            _ => return,
        };
        *prop = id;
    }

    pub fn rebuild_collections(&mut self) -> Result<(), Box<dyn Error>> {
        let (state, collections) = with_preset_db(|db| db.build_collections(&self.runtime_state))??;
        self.runtime_state = state;
        self.collections = collections;
        Ok(())
    }

    pub fn count_filter_items(&self, kind: PotFilterItemKind) -> u32 {
        use PotFilterItemKind::*;
        let collections = &self.collections.filter_item_collections;
        let len = match kind {
            Database => collections.databases.len(),
            NksBank => collections.nks.banks.len(),
            NksSubBank => collections.nks.sub_banks.len(),
            NksCategory => collections.nks.categories.len(),
            NksSubCategory => collections.nks.sub_categories.len(),
            NksMode => collections.nks.modes.len(),
        };
        len as _
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
        Some(self.find_filter_item_at_index(kind, index)?.id)
    }

    fn find_filter_item_at_index(
        &self,
        kind: PotFilterItemKind,
        index: u32,
    ) -> Option<&FilterItem> {
        use PotFilterItemKind::*;
        let collections = &self.collections.filter_item_collections;
        let index = index as usize;
        match kind {
            Database => collections.databases.get(index),
            NksBank => collections.nks.banks.get(index),
            NksSubBank => collections.nks.sub_banks.get(index),
            NksCategory => collections.nks.categories.get(index),
            NksSubCategory => collections.nks.sub_categories.get(index),
            NksMode => collections.nks.modes.get(index),
        }
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
        use PotFilterItemKind::*;
        fn find(items: &[FilterItem], id: FilterItemId) -> Option<(u32, &FilterItem)> {
            let (i, item) = items.iter().enumerate().find(|(_, item)| item.id == id)?;
            Some((i as u32, item))
        }
        let collections = &self.collections.filter_item_collections;
        match kind {
            Database => find(&collections.databases, id),
            NksBank => find(&collections.nks.banks, id),
            NksSubBank => find(&collections.nks.sub_banks, id),
            NksCategory => find(&collections.nks.categories, id),
            NksSubCategory => find(&collections.nks.sub_categories, id),
            NksMode => find(&collections.nks.modes, id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FilterItem {
    pub persistent_id: String,
    pub id: FilterItemId,
    pub parent_name: String,
    pub name: String,
}

#[derive(Debug)]
pub struct Preset {
    pub favorite_id: String,
    pub id: PresetId,
    pub name: String,
    pub file_name: PathBuf,
    pub file_ext: String,
}

#[derive(serde::Deserialize)]
struct ParamAssignment {
    id: Option<u32>,
}
