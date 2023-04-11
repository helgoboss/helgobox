//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use crate::base::blocking_lock;
use crate::domain::pot::nks::{NksFile, NksFilterSettings, PersistentNksFilterSettings};
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::{Fx, Reaper};
use reaper_medium::InsertMediaMode;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod nks;
pub mod worker;

pub type FilterItemId = nks::FilterItemId;
pub type PresetId = nks::PresetId;
pub type PresetDb = nks::PresetDb;

pub fn with_preset_db<R>(f: impl FnOnce(&PresetDb) -> R) -> Result<R, &'static str> {
    nks::with_preset_db(f)
}

pub fn preset_db() -> Result<&'static Mutex<PresetDb>, &'static str> {
    nks::preset_db()
}

pub type SharedRuntimePotUnit = Arc<Mutex<RuntimePotUnit>>;

#[derive(Debug)]
pub enum PotUnit {
    Unloaded {
        state: PersistentState,
        previous_load_error: &'static str,
    },
    Loaded(SharedRuntimePotUnit),
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

    pub fn loaded(&mut self) -> Result<SharedRuntimePotUnit, &'static str> {
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
            PotUnit::Loaded(p) => Ok(p.clone()),
        }
    }

    pub fn persistent_state(&self) -> PersistentState {
        match self {
            PotUnit::Unloaded { state, .. } => state.clone(),
            PotUnit::Loaded(u) => blocking_lock(u).persistent_state(),
        }
    }
}

#[derive(Debug)]
pub struct RuntimePotUnit {
    pub runtime_state: RuntimeState,
    pub collections: Collections,
    pub stats: Stats,
}

#[derive(Debug, Default)]
pub struct Stats {
    pub query_duration: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeState {
    filter_settings: FilterSettings,
    search_expression: String,
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
                search_expression: "".to_string(),
                preset_id,
            }
        })
    }

    pub fn search_expression_mut(&mut self) -> &mut String {
        &mut self.search_expression
    }

    pub fn filter_item_id_mut(&mut self, kind: PotFilterItemKind) -> &mut Option<FilterItemId> {
        use PotFilterItemKind::*;
        let settings = &mut self.filter_settings.nks;
        match kind {
            NksBank => &mut settings.bank,
            NksSubBank => &mut settings.sub_bank,
            NksCategory => &mut settings.category,
            NksSubCategory => &mut settings.sub_category,
            NksMode => &mut settings.mode,
            _ => panic!("unsupported filter item ID"),
        }
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

#[derive(Clone, Debug, Default)]
pub struct FilterSettings {
    pub nks: nks::NksFilterSettings,
}

#[derive(Debug, Default)]
pub struct Collections {
    filter_item_collections: FilterItemCollections,
    preset_collection: PresetCollection,
}

impl Collections {
    pub fn find_all_filter_items(&self, kind: PotFilterItemKind) -> &[FilterItem] {
        use PotFilterItemKind::*;
        let collections = &self.filter_item_collections;
        match kind {
            Database => &collections.databases,
            NksBank => &collections.nks.banks,
            NksSubBank => &collections.nks.sub_banks,
            NksCategory => &collections.nks.categories,
            NksSubCategory => &collections.nks.sub_categories,
            NksMode => &collections.nks.modes,
        }
    }
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
    pub fn load(state: &PersistentState) -> Result<SharedRuntimePotUnit, &'static str> {
        let mut unit = Self {
            runtime_state: RuntimeState::load(state)?,
            collections: Default::default(),
            stats: Default::default(),
        };
        let shared_unit = Arc::new(Mutex::new(unit));
        // TODO-high CONTINUE
        // unit.rebuild_collections()
        //     .map_err(|_| "couldn't rebuild collections on load")?;
        Ok(shared_unit)
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
        *self.runtime_state.filter_item_id_mut(kind) = id;
    }

    pub fn rebuild_collections(&mut self) -> Result<(), Box<dyn Error>> {
        let before = Instant::now();
        let (state, collections) = with_preset_db(|db| db.build_collections(&self.runtime_state))??;
        self.runtime_state = state;
        self.collections = collections;
        self.stats.query_duration = before.elapsed();
        Ok(())
    }

    pub fn count_filter_items(&self, kind: PotFilterItemKind) -> u32 {
        self.collections.find_all_filter_items(kind).len() as u32
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

    pub fn find_filter_item_at_index(
        &self,
        kind: PotFilterItemKind,
        index: u32,
    ) -> Option<&FilterItem> {
        self.collections
            .find_all_filter_items(kind)
            .get(index as usize)
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
    /// Only set for sub filters. If not set, we know it's a top-level filter.
    pub parent_name: Option<String>,
    /// If not set, parent name should be set. It's the most unspecific sub filter of a
    /// top-level filter, so to say.
    pub name: Option<String>,
}

impl FilterItem {
    pub fn effective_leaf_name(&self) -> Cow<str> {
        match &self.name {
            None => match &self.parent_name {
                None => "".into(),
                Some(n) => format!("{n}*").into(),
            },
            Some(n) => n.into(),
        }
    }
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

pub fn load_preset(preset: &Preset, fx: &Fx) -> Result<CurrentPreset, &'static str> {
    match preset.file_ext.as_str() {
        "wav" | "aif" => load_audio_preset(&preset, fx),
        "nksf" | "nksfx" => load_nksf_preset(&preset, fx),
        _ => Err("unsupported preset format"),
    }
}

fn load_nksf_preset(preset: &Preset, fx: &Fx) -> Result<CurrentPreset, &'static str> {
    let nks_file = NksFile::load(&preset.file_name)?;
    let nks_content = nks_file.content()?;
    make_sure_fx_has_correct_type(nks_content.vst_magic_number, fx)?;
    fx.set_vst_chunk(nks_content.vst_chunk)?;
    Ok(nks_content.current_preset)
}

fn load_audio_preset(preset: &Preset, fx: &Fx) -> Result<CurrentPreset, &'static str> {
    const RS5K_VST_ID: u32 = 1920167789;
    make_sure_fx_has_correct_type(RS5K_VST_ID, fx)?;
    let window_is_open_before = fx.window_is_open();
    if window_is_open_before {
        if !fx.window_has_focus() {
            fx.hide_floating_window();
            fx.show_in_floating_window();
        }
    } else {
        fx.show_in_floating_window();
    }
    load_media_in_last_focused_rs5k(&preset.file_name)?;
    if !window_is_open_before {
        fx.hide_floating_window();
    }
    Ok(CurrentPreset::default())
}

fn make_sure_fx_has_correct_type(vst_magic_number: u32, fx: &Fx) -> Result<(), &'static str> {
    if !fx.is_available() {
        return Err("FX not available");
    }
    let fx_info = fx.info()?;
    if fx_info.id != vst_magic_number.to_string() {
        // We don't have the right plug-in type. Remove FX and insert correct one.
        let chain = fx.chain();
        let fx_index = fx.index();
        chain.remove_fx(fx)?;
        // Need to put some random string in front of "<" due to bug in REAPER < 6.69,
        // otherwise loading by VST2 magic number doesn't work.
        chain.insert_fx_by_name(fx_index, format!("i7zh34z<{vst_magic_number}"));
    }
    Ok(())
}

fn load_media_in_last_focused_rs5k(path: &Path) -> Result<(), &'static str> {
    Reaper::get().medium_reaper().insert_media(
        path,
        InsertMediaMode::CurrentReasamplomatic,
        Default::default(),
    )?;
    Ok(())
}
