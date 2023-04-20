//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use crate::base::{blocking_lock, blocking_lock_arc, NamedChannelSender, SenderToNormalThread};
use crate::domain::pot::nks::{Filters, NksFile, OptFilter, PersistentNksFilterSettings, PluginId};
use crate::domain::{BackboneState, InstanceStateChanged, PotStateChangedEvent, SoundPlayer};
use enum_map::EnumMap;
use enumset::EnumSet;
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::{Fx, FxChain, FxChainContext, Project, Reaper, Track};
use reaper_medium::{InsertMediaMode, MasterTrackBehavior, ReaperVolumeValue};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod nks;
mod worker;

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

    pub fn loaded(
        &mut self,
        sender: &SenderToNormalThread<InstanceStateChanged>,
    ) -> Result<SharedRuntimePotUnit, &'static str> {
        match self {
            PotUnit::Unloaded {
                state,
                previous_load_error,
            } => {
                if !previous_load_error.is_empty() {
                    return Err(previous_load_error);
                }
                match RuntimePotUnit::load(state, sender.clone()) {
                    Ok(u) => {
                        *self = Self::Loaded(u.clone());
                        Ok(u)
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
    pub wasted_runs: u32,
    pub wasted_duration: Duration,
    pub stats: Stats,
    sender: SenderToNormalThread<InstanceStateChanged>,
    change_counter: u64,
    sound_player: SoundPlayer,
    preview_volume: ReaperVolumeValue,
    pub destination_descriptor: DestinationDescriptor,
    pub name_track_after_preset: bool,
    show_excluded_filter_items: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct DestinationDescriptor {
    pub track: DestinationTrackDescriptor,
    pub fx_index: u32,
}

#[derive(Copy, Clone, Debug, Default)]
pub enum DestinationTrackDescriptor {
    #[default]
    SelectedTrack,
    MasterTrack,
    Track(u32),
}

impl DestinationTrackDescriptor {
    pub fn resolve(&self, project: Project) -> Result<Track, &'static str> {
        let track = match self {
            DestinationTrackDescriptor::SelectedTrack => project
                .first_selected_track(MasterTrackBehavior::IncludeMasterTrack)
                .ok_or("No track selected")?,
            DestinationTrackDescriptor::MasterTrack => project
                .master_track()
                .map_err(|_| "Couldn't get master track")?,
            DestinationTrackDescriptor::Track(i) => project
                .track_by_index(*i)
                .ok_or("No track at that position")?,
        };
        Ok(track)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::SelectedTrack)
    }
}

pub enum DestinationInstruction {
    Existing(Destination),
    AddTrack,
}

impl DestinationInstruction {
    pub fn get_existing_or_create(self) -> Result<Destination, &'static str> {
        match self {
            DestinationInstruction::Existing(d) => Ok(d),
            DestinationInstruction::AddTrack => {
                let track = Reaper::get().current_project().add_track()?;
                track.select_exclusively();
                let dest = Destination {
                    chain: track.normal_fx_chain(),
                    fx_index: 0,
                };
                Ok(dest)
            }
        }
    }

    pub fn get_existing(&self) -> Option<&Destination> {
        match self {
            DestinationInstruction::Existing(d) => Some(d),
            DestinationInstruction::AddTrack => None,
        }
    }
}

impl DestinationDescriptor {
    pub fn resolve_destination(&self) -> Result<DestinationInstruction, &'static str> {
        let project = Reaper::get().current_project();
        if let Ok(track) = self.track.resolve(project) {
            let dest = Destination {
                chain: track.normal_fx_chain(),
                fx_index: self.fx_index,
            };
            Ok(DestinationInstruction::Existing(dest))
        } else {
            Ok(DestinationInstruction::AddTrack)
        }
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub query_duration: Duration,
}

pub struct BuildInput<'a> {
    pub state: &'a RuntimeState,
    pub change_hint: Option<ChangeHint>,
    pub filter_exclude_list: PotFilterExcludeList,
}

impl<'a> BuildInput<'a> {
    pub fn affected_kinds(&self) -> EnumSet<PotFilterItemKind> {
        match self.change_hint {
            None => EnumSet::all(),
            Some(ChangeHint::SearchExpression) => EnumSet::empty(),
            Some(ChangeHint::Filter(changed_kind)) => changed_kind.dependent_kinds().collect(),
            Some(ChangeHint::FilterExclude) => EnumSet::all(),
        }
    }
}

#[derive(Copy, Clone)]
pub enum ChangeHint {
    Filter(PotFilterItemKind),
    SearchExpression,
    FilterExclude,
}

pub struct BuildOutput {
    pub collections: Collections,
    pub stats: Stats,
    pub filter_settings: FilterSettings,
    pub changed_filter_item_kinds: EnumSet<PotFilterItemKind>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeState {
    filter_settings: FilterSettings,
    pub search_expression: String,
    pub use_wildcard_search: bool,
    preset_id: Option<PresetId>,
}

impl RuntimeState {
    pub fn load(persistent_state: &PersistentState) -> Result<Self, &'static str> {
        with_preset_db(|db| {
            let filter_exclude_list = BackboneState::get().pot_filter_exclude_list();
            let collections = db.build_filter_items(
                &Default::default(),
                EnumSet::all().into_iter(),
                &filter_exclude_list,
            );
            let filter_settings = {
                let nks = &persistent_state.filter_settings.nks;
                let mut filters = Filters::empty();
                let mut set_filter = |setting: &Option<String>, kind: PotFilterItemKind| {
                    let id = setting.as_ref().and_then(|persistent_id| {
                        let item = collections
                            .get(kind)
                            .iter()
                            .find(|item| &item.persistent_id == persistent_id)?;
                        Some(item.id)
                    });
                    filters.set(kind, id);
                };
                set_filter(&nks.bank, PotFilterItemKind::NksBank);
                set_filter(&nks.sub_bank, PotFilterItemKind::NksSubBank);
                set_filter(&nks.category, PotFilterItemKind::NksCategory);
                set_filter(&nks.sub_category, PotFilterItemKind::NksSubCategory);
                set_filter(&nks.mode, PotFilterItemKind::NksMode);
                FilterSettings { nks: filters }
            };
            let preset_id = persistent_state
                .preset_id
                .as_ref()
                .and_then(|persistent_id| db.find_preset_id_by_favorite_id(persistent_id));
            Self {
                filter_settings,
                search_expression: "".to_string(),
                use_wildcard_search: false,
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

#[derive(Clone, Debug, Default)]
pub struct FilterSettings {
    pub nks: nks::Filters,
}

#[derive(Debug, Default)]
pub struct Collections {
    filter_item_collections: FilterItemCollections,
    preset_collection: PresetCollection,
}

impl Collections {
    pub fn find_all_filter_items(&self, kind: PotFilterItemKind) -> &[FilterItem] {
        if kind == PotFilterItemKind::Database {
            return &self.filter_item_collections.databases;
        }
        self.filter_item_collections.nks.get(kind)
    }
}

#[derive(Debug)]
pub struct CurrentPreset {
    preset: Preset,
    macro_param_banks: Vec<MacroParamBank>,
}

#[derive(Debug)]
pub struct MacroParamBank {
    params: Vec<MacroParam>,
}

impl MacroParamBank {
    pub fn new(params: Vec<MacroParam>) -> Self {
        Self { params }
    }

    pub fn name(&self) -> String {
        let mut name = String::with_capacity(32);
        for p in &self.params {
            if !p.section_name.is_empty() {
                if !name.is_empty() {
                    name += " / ";
                }
                name += &p.section_name;
            }
        }
        name
    }

    pub fn find_macro_param_at(&self, slot_index: u32) -> Option<&MacroParam> {
        self.params.get(slot_index as usize)
    }

    pub fn param_count(&self) -> u32 {
        self.params.len() as _
    }
}

#[derive(Clone, Debug)]
pub struct MacroParam {
    pub name: String,
    pub section_name: String,
    pub param_index: Option<u32>,
}

impl CurrentPreset {
    pub fn without_parameters(preset: Preset) -> Self {
        Self {
            preset,
            macro_param_banks: Default::default(),
        }
    }

    pub fn with_parameters(preset: Preset, macro_param_banks: Vec<MacroParamBank>) -> Self {
        Self {
            preset,
            macro_param_banks,
        }
    }

    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    pub fn find_macro_param_bank_at(&self, bank_index: u32) -> Option<&MacroParamBank> {
        self.macro_param_banks.get(bank_index as usize)
    }

    pub fn find_macro_param_at(&self, slot_index: u32) -> Option<&MacroParam> {
        let bank_index = slot_index / 8;
        let bank_slot_index = slot_index % 8;
        self.find_bank_macro_param_at(bank_index, bank_slot_index)
    }

    pub fn find_bank_macro_param_at(
        &self,
        bank_index: u32,
        bank_slot_index: u32,
    ) -> Option<&MacroParam> {
        self.macro_param_banks
            .get(bank_index as usize)?
            .find_macro_param_at(bank_slot_index)
    }

    pub fn macro_param_bank_count(&self) -> u32 {
        self.macro_param_banks.len() as _
    }

    pub fn has_params(&self) -> bool {
        self.macro_param_banks.len() > 0
    }
}

impl RuntimePotUnit {
    pub fn load(
        state: &PersistentState,
        sender: SenderToNormalThread<InstanceStateChanged>,
    ) -> Result<SharedRuntimePotUnit, &'static str> {
        let sound_player = SoundPlayer::new();
        let unit = Self {
            runtime_state: RuntimeState::load(state)?,
            collections: Default::default(),
            wasted_runs: 0,
            wasted_duration: Default::default(),
            stats: Default::default(),
            sender,
            change_counter: 0,
            preview_volume: sound_player.volume().unwrap_or_default(),
            sound_player,
            destination_descriptor: Default::default(),
            name_track_after_preset: true,
            show_excluded_filter_items: false,
        };
        let shared_unit = Arc::new(Mutex::new(unit));
        blocking_lock_arc(&shared_unit).rebuild_collections(shared_unit.clone(), None);
        Ok(shared_unit)
    }

    pub fn preview_volume(&self) -> ReaperVolumeValue {
        self.preview_volume
    }

    pub fn set_preview_volume(&mut self, volume: ReaperVolumeValue) {
        self.preview_volume = volume;
        self.sound_player
            .set_volume(volume)
            .expect("changing preview volume value failed");
    }

    pub fn persistent_state(&self) -> PersistentState {
        let nks_settings = &self.runtime_state.filter_settings.nks;
        let nks_items = &self.collections.filter_item_collections.nks;
        let find_id = |kind: PotFilterItemKind| {
            nks_settings.get(kind).and_then(|id| {
                nks_items
                    .get(kind)
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.persistent_id.clone())
            })
        };
        let filter_settings = PersistentFilterSettings {
            nks: PersistentNksFilterSettings {
                bank: find_id(PotFilterItemKind::NksBank),
                sub_bank: find_id(PotFilterItemKind::NksSubBank),
                category: find_id(PotFilterItemKind::NksCategory),
                sub_category: find_id(PotFilterItemKind::NksSubCategory),
                mode: find_id(PotFilterItemKind::NksMode),
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

    pub fn play_preview(&mut self, preset_id: PresetId) -> Result<(), &'static str> {
        let preview_file = with_preset_db(|db| db.find_preset_preview_file(preset_id))?
            .ok_or("couldn't find preset or build preset preview file")?;
        self.sound_player.load_file(&preview_file)?;
        self.sound_player.play()?;
        Ok(())
    }

    pub fn stop_preview(&mut self) -> Result<(), &'static str> {
        self.sound_player.stop()
    }

    pub fn preset(&self) -> Option<Preset> {
        let preset_id = self.preset_id()?;
        with_preset_db(|db| db.find_preset_by_id(preset_id)).ok()?
    }

    pub fn load_preset(
        &self,
        preset: &Preset,
        options: LoadPresetOptions,
    ) -> Result<(), &'static str> {
        let dest = self.resolve_destination()?.get_existing_or_create()?;
        self.load_preset_at(preset, &dest, options)?;
        if self.name_track_after_preset {
            if let Some(track) = dest.chain.track() {
                track.set_name(preset.name.as_str());
            }
        }
        Ok(())
    }

    pub fn resolve_destination(&self) -> Result<DestinationInstruction, &'static str> {
        self.destination_descriptor.resolve_destination()
    }

    pub fn load_preset_at(
        &self,
        preset: &Preset,
        destination: &Destination,
        options: LoadPresetOptions,
    ) -> Result<(), &'static str> {
        let outcome = match preset.file_ext.as_str() {
            "wav" | "aif" => load_audio_preset(&preset, destination, options)?,
            "nksf" | "nksfx" => load_nksf_preset(&preset, destination, options)?,
            _ => return Err("unsupported preset format"),
        };
        BackboneState::target_state()
            .borrow_mut()
            .set_current_fx_preset(outcome.fx, outcome.current_preset);
        Ok(())
    }

    pub fn state(&self) -> &RuntimeState {
        &self.runtime_state
    }

    pub fn preset_id(&self) -> Option<PresetId> {
        self.runtime_state.preset_id
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.runtime_state.preset_id = id;
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::PresetChanged { id },
            ));
    }

    pub fn get_filter(&self, kind: PotFilterItemKind) -> OptFilter {
        self.runtime_state.filter_settings.nks.get(kind)
    }

    pub fn filter_is_set_to_non_none(&self, kind: PotFilterItemKind) -> bool {
        matches!(self.get_filter(kind), Some(nks::FilterItemId(Some(_))))
    }

    pub fn show_excluded_filter_items(&self) -> bool {
        self.show_excluded_filter_items
    }

    pub fn set_show_excluded_filter_items(
        &mut self,
        show: bool,
        shared_self: SharedRuntimePotUnit,
    ) {
        self.show_excluded_filter_items = show;
        self.rebuild_collections(shared_self, Some(ChangeHint::FilterExclude));
    }

    pub fn include_filter_item(
        &mut self,
        kind: PotFilterItemKind,
        id: FilterItemId,
        include: bool,
        shared_self: SharedRuntimePotUnit,
    ) {
        {
            let mut list = BackboneState::get().pot_filter_exclude_list_mut();
            if include {
                list.include(kind, id);
            } else {
                list.exclude(kind, id);
            }
        }
        self.rebuild_collections(shared_self, Some(ChangeHint::FilterExclude));
    }

    pub fn set_filter(
        &mut self,
        kind: PotFilterItemKind,
        id: OptFilter,
        shared_self: SharedRuntimePotUnit,
    ) {
        self.runtime_state.filter_settings.nks.set(kind, id);
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::FilterItemChanged { kind, filter: id },
            ));
        self.rebuild_collections(shared_self, Some(ChangeHint::Filter(kind)));
    }

    pub fn rebuild_collections(
        &mut self,
        shared_self: SharedRuntimePotUnit,
        change_hint: Option<ChangeHint>,
    ) {
        // Acquire exclude list in main thread
        let filter_exclude_list = if self.show_excluded_filter_items {
            PotFilterExcludeList::default()
        } else {
            BackboneState::get().pot_filter_exclude_list().clone()
        };
        let mut runtime_state = self.runtime_state.clone();
        // Here we already have enough knowledge to fix some filter settings.
        runtime_state
            .filter_settings
            .nks
            .clear_excluded_ones(&filter_exclude_list);
        // Spawn new async task (don't block GUI thread, might take longer)
        self.change_counter += 1;
        let last_change_counter = self.change_counter;
        worker::spawn(async move {
            // Debounce (cheap)
            // If we remove this, the wasted runs will increase when quickly changing filters
            // (via encoder).
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let pot_unit = blocking_lock_arc(&shared_self);
                if pot_unit.change_counter != last_change_counter {
                    return Ok(());
                }
            }
            // Build (expensive)
            let build_input = BuildInput {
                state: &runtime_state,
                change_hint,
                filter_exclude_list,
            };
            let build_outcome = with_preset_db(|db| db.build_collections(build_input))??;
            // Set result (cheap)
            // Only set result if no new build has been requested in the meantime.
            // Prevents flickering and increment/decrement issues.
            let mut pot_unit = blocking_lock_arc(&shared_self);
            if pot_unit.change_counter != last_change_counter {
                pot_unit.wasted_duration += build_outcome.stats.query_duration;
                pot_unit.wasted_runs += 1;
                return Ok(());
            }
            pot_unit.notify_build_outcome_ready(build_outcome);
            Ok(())
        });
    }

    fn notify_build_outcome_ready(&mut self, build_outcome: BuildOutput) {
        self.collections.preset_collection = build_outcome.collections.preset_collection;
        for (kind, collection) in build_outcome
            .collections
            .filter_item_collections
            .nks
            .into_iter()
        {
            if build_outcome.changed_filter_item_kinds.contains(kind) {
                self.collections
                    .filter_item_collections
                    .nks
                    .set(kind, collection);
            }
        }
        for changed_kind in build_outcome.changed_filter_item_kinds {
            self.runtime_state.filter_settings.nks.set(
                changed_kind,
                build_outcome.filter_settings.nks.get(changed_kind),
            );
        }
        self.stats = build_outcome.stats;
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::IndexesRebuilt,
            ));
    }

    pub fn count_filter_items(&self, kind: PotFilterItemKind) -> u32 {
        self.collections.find_all_filter_items(kind).len() as u32
    }

    pub fn preset_count(&self) -> u32 {
        self.collections.preset_collection.len() as u32
    }

    pub fn find_next_preset_index(&self, amount: i32) -> Option<u32> {
        let preset_count = self.preset_count();
        if preset_count == 0 {
            return None;
        }
        match self
            .runtime_state
            .preset_id
            .and_then(|id| self.find_index_of_preset(id))
        {
            None => {
                if amount < 0 {
                    Some(preset_count - 1)
                } else {
                    Some(0)
                }
            }
            Some(current_index) => {
                let next_index = current_index as i32 + amount;
                if next_index < 0 {
                    Some(preset_count - 1)
                } else if next_index as u32 >= preset_count {
                    Some(0)
                } else {
                    Some(next_index as u32)
                }
            }
        }
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
        if kind == Database {
            find(&collections.databases, id)
        } else {
            find(collections.nks.get(kind), id)
        }
    }
}

#[derive(Clone, Debug)]
pub struct FilterItem {
    // TODO-high-pot Distinguish <Any> and <None> in persistence
    pub persistent_id: String,
    /// `None` is also a valid filter item! It would match filter `<None>` (e.g. no category
    /// assigned at all)
    pub id: FilterItemId,
    /// Only set for sub filters. If not set, we know it's a top-level filter.
    pub parent_name: Option<String>,
    /// If not set, parent name should be set. It's the most unspecific sub filter of a
    /// top-level filter, so to say.
    pub name: Option<String>,
}

impl FilterItem {
    pub fn none() -> Self {
        Self {
            // TODO-high-pot Persistence
            persistent_id: "".to_string(),
            id: nks::FilterItemId(None),
            parent_name: None,
            name: Some("<None>".to_string()),
        }
    }

    pub fn simple(id: u32, name: &str) -> Self {
        Self {
            // TODO-high-pot Persistence
            persistent_id: "".to_string(),
            id: nks::FilterItemId(Some(id)),
            parent_name: None,
            name: Some(name.to_string()),
        }
    }

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

#[derive(Clone, Debug)]
pub struct Preset {
    pub favorite_id: String,
    pub id: PresetId,
    pub name: String,
    pub file_name: PathBuf,
    pub file_ext: String,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ParamAssignment {
    id: Option<u32>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    autoname: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    vflag: bool,
}

fn load_nksf_preset(
    preset: &Preset,
    destination: &Destination,
    options: LoadPresetOptions,
) -> Result<LoadPresetOutcome, &'static str> {
    let nks_file = NksFile::load(&preset.file_name)?;
    let nks_content = nks_file.content()?;
    let existing_fx = destination.resolve();
    let fx_was_open_before = existing_fx
        .as_ref()
        .map(|fx| fx.window_is_open())
        .unwrap_or(false);
    let output = ensure_fx_has_correct_type(nks_content.plugin_id, destination, existing_fx)?;
    output.fx.set_vst_chunk(nks_content.vst_chunk)?;
    options
        .window_behavior
        .open_or_close(&output.fx, fx_was_open_before, output.op);
    let outcome = LoadPresetOutcome {
        fx: output.fx,
        current_preset: CurrentPreset::with_parameters(
            preset.clone(),
            nks_content.macro_param_banks,
        ),
    };
    Ok(outcome)
}

fn load_audio_preset(
    preset: &Preset,
    destination: &Destination,
    options: LoadPresetOptions,
) -> Result<LoadPresetOutcome, &'static str> {
    const RS5K_VST_ID: u32 = 1920167789;
    let plugin_id = PluginId::Vst2 {
        vst_magic_number: RS5K_VST_ID,
    };
    let existing_fx = destination.resolve();
    let fx_was_open_before = existing_fx
        .as_ref()
        .map(|fx| fx.window_is_open())
        .unwrap_or(false);
    let output = ensure_fx_has_correct_type(plugin_id, destination, existing_fx)?;
    // Make sure RS5k has focus
    let window_is_open_now = output.fx.window_is_open();
    if window_is_open_now {
        if !output.fx.window_has_focus() {
            output.fx.hide_floating_window();
            output.fx.show_in_floating_window();
        }
    } else {
        output.fx.show_in_floating_window();
    }
    // Load into RS5k
    load_media_in_last_focused_rs5k(&preset.file_name)?;
    // Remainder
    options
        .window_behavior
        .open_or_close(&output.fx, fx_was_open_before, output.op);
    let outcome = LoadPresetOutcome {
        fx: output.fx,
        current_preset: CurrentPreset::without_parameters(preset.clone()),
    };
    Ok(outcome)
}

struct LoadPresetOutcome {
    fx: Fx,
    current_preset: CurrentPreset,
}

pub struct FxEnsureOutput {
    pub fx: Fx,
    pub op: FxEnsureOp,
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum FxEnsureOp {
    Same,
    Added,
    Replaced,
}

fn ensure_fx_has_correct_type(
    plugin_id: PluginId,
    destination: &Destination,
    existing_fx: Option<Fx>,
) -> Result<FxEnsureOutput, &'static str> {
    let output = match existing_fx {
        None => {
            let fx = insert_fx_by_plugin_id(plugin_id, destination)?;
            FxEnsureOutput {
                fx,
                op: FxEnsureOp::Added,
            }
        }
        Some(fx) => {
            let fx_info = fx.info()?;
            if fx_info.id == plugin_id.formatted_for_reaper() {
                FxEnsureOutput {
                    fx,
                    op: FxEnsureOp::Same,
                }
            } else {
                // We don't have the right plug-in type. Remove FX and insert correct one.
                destination.chain.remove_fx(&fx)?;
                let fx = insert_fx_by_plugin_id(plugin_id, destination)?;
                FxEnsureOutput {
                    fx,
                    op: FxEnsureOp::Replaced,
                }
            }
        }
    };
    Ok(output)
}

fn insert_fx_by_plugin_id(
    plugin_id: PluginId,
    destination: &Destination,
) -> Result<Fx, &'static str> {
    // Need to put some random string in front of "<" due to bug in REAPER < 6.69,
    // otherwise loading by VST2 magic number doesn't work.
    let name = format!(
        "i7zh34z{}{}",
        plugin_id.reaper_prefix(),
        plugin_id.formatted_for_reaper()
    );
    destination
        .chain
        .insert_fx_by_name(destination.fx_index, name)
        .ok_or("couldn't insert FX by VST magic number")
}

fn load_media_in_last_focused_rs5k(path: &Path) -> Result<(), &'static str> {
    Reaper::get().medium_reaper().insert_media(
        path,
        InsertMediaMode::CurrentReasamplomatic,
        Default::default(),
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct Destination {
    pub chain: FxChain,
    pub fx_index: u32,
}

impl Destination {
    pub fn resolve(&self) -> Option<Fx> {
        self.chain.fx_by_index(self.fx_index)
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct LoadPresetOptions {
    pub window_behavior: LoadPresetWindowBehavior,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum LoadPresetWindowBehavior {
    NeverShow,
    #[default]
    ShowOnlyIfPreviouslyShown,
    ShowOnlyIfPreviouslyShownOrNewlyAdded,
    AlwaysShow,
}

impl LoadPresetWindowBehavior {
    pub fn open_or_close(&self, fx: &Fx, was_open_before: bool, op: FxEnsureOp) {
        let now_is_open = fx.window_is_open();
        match self {
            LoadPresetWindowBehavior::NeverShow => {
                if now_is_open {
                    fx.hide_floating_window();
                }
            }
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShown => {
                if !was_open_before && now_is_open {
                    fx.hide_floating_window();
                } else if was_open_before && !now_is_open {
                    fx.show_in_floating_window();
                }
            }
            LoadPresetWindowBehavior::AlwaysShow => {
                if !now_is_open {
                    fx.show_in_floating_window();
                }
            }
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded => {
                if op == FxEnsureOp::Added {
                    if !now_is_open {
                        fx.show_in_floating_window()
                    }
                } else if !was_open_before && now_is_open {
                    fx.hide_floating_window();
                } else if was_open_before && !now_is_open {
                    fx.show_in_floating_window();
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PotFilterExcludeList {
    exluded_items: EnumMap<PotFilterItemKind, HashSet<FilterItemId>>,
}

impl PotFilterExcludeList {
    pub fn contains(&self, kind: PotFilterItemKind, id: FilterItemId) -> bool {
        self.exluded_items[kind].contains(&id)
    }

    pub fn include(&mut self, kind: PotFilterItemKind, id: FilterItemId) {
        self.exluded_items[kind].remove(&id);
    }

    pub fn exclude(&mut self, kind: PotFilterItemKind, id: FilterItemId) {
        self.exluded_items[kind].insert(id);
    }

    pub fn normal_excludes_by_kind(
        &self,
        kind: PotFilterItemKind,
    ) -> impl Iterator<Item = &u32> + '_ {
        self.exluded_items[kind]
            .iter()
            .filter_map(|id| Some(id.0.as_ref()?))
    }

    pub fn contains_none(&self, kind: PotFilterItemKind) -> bool {
        self.exluded_items[kind].contains(&FilterItemId::NONE)
    }
}
