//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use crate::domain::pot::nks::FilterItemId;
use enum_map::EnumMap;
use realearn_api::persistence::PotFilterItemKind;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub mod nks;

pub type PresetId = nks::PresetId;
pub type PresetDb = nks::PresetDb;

pub fn with_preset_db<R>(f: impl FnOnce(&PresetDb) -> R) -> Result<R, &'static str> {
    nks::with_preset_db(f)
}

pub fn preset_db() -> Result<&'static Mutex<PresetDb>, &'static str> {
    nks::preset_db()
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct NavigationState {
    filter_item_ids: EnumMap<PotFilterItemKind, Option<FilterItemId>>,
    preset_id: Option<PresetId>,
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
        self.preset_id
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.preset_id = id;
    }

    pub fn filter_item_id(&self, kind: PotFilterItemKind) -> Option<FilterItemId> {
        self.filter_item_ids[kind]
    }

    pub fn set_filter_item_id(&mut self, kind: PotFilterItemKind, id: Option<FilterItemId>) {
        self.filter_item_ids[kind] = id;
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
