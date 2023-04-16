//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use crate::base::{blocking_lock, blocking_lock_arc, NamedChannelSender, SenderToNormalThread};
use crate::domain::pot::nks::{Filters, NksFile, OptFilter, PersistentNksFilterSettings, PluginId};
use crate::domain::{BackboneState, InstanceStateChanged, PotStateChangedEvent, SoundPlayer};
use indexmap::IndexSet;
use realearn_api::persistence::PotFilterItemKind;
use reaper_high::{Fx, FxChain, FxChainContext, Reaper};
use reaper_medium::{InsertMediaMode, MasterTrackBehavior, ReaperVolumeValue};
use std::borrow::Cow;
use std::collections::HashMap;
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
}

#[derive(Debug, Default)]
pub struct Stats {
    pub query_duration: Duration,
}

pub struct BuildOutcome {
    pub collections: Collections,
    pub stats: Stats,
    pub filter_settings: FilterSettings,
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
            let filter_settings = db
                .build_filter_items(&Default::default())
                .map(|collections| {
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
                        nks: Filters {
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
                use_wildcard_search: false,
                preset_id,
            }
        })
    }

    fn get_filter_mut(&mut self, kind: PotFilterItemKind) -> &mut OptFilter {
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
    pub nks: nks::Filters,
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

#[derive(Debug)]
pub struct CurrentPreset {
    preset: Preset,
    macro_params: HashMap<u32, MacroParam>,
}

#[derive(Clone, Debug)]
pub struct MacroParam {
    pub name: String,
    pub section_name: String,
    pub param_index: u32,
}

impl CurrentPreset {
    pub fn without_parameters(preset: Preset) -> Self {
        Self {
            preset,
            macro_params: Default::default(),
        }
    }

    pub fn with_parameters(preset: Preset, macro_params: HashMap<u32, MacroParam>) -> Self {
        Self {
            preset,
            macro_params,
        }
    }

    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    pub fn find_macro_param_at(&self, slot_index: u32) -> Option<&MacroParam> {
        self.macro_params.get(&slot_index)
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
        };
        let shared_unit = Arc::new(Mutex::new(unit));
        blocking_lock_arc(&shared_unit).rebuild_collections(shared_unit.clone());
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
        let find_id = |setting: OptFilter, items: &[FilterItem]| {
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

    pub fn load_preset(&self, preset: &Preset) -> Result<(), &'static str> {
        let dest = self.preset_load_destination()?;
        self.load_preset_at(preset, &dest)?;
        Ok(())
    }

    pub fn preset_load_destination(&self) -> Result<PresetLoadDestination, &'static str> {
        PresetLoadDestination::first_fx_on_selected_track()
    }

    pub fn load_preset_at(
        &self,
        preset: &Preset,
        destination: &PresetLoadDestination,
    ) -> Result<(), &'static str> {
        let outcome = match preset.file_ext.as_str() {
            "wav" | "aif" => load_audio_preset(&preset, destination)?,
            "nksf" | "nksfx" => load_nksf_preset(&preset, destination)?,
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

    pub fn filter_is_set_to_non_none(&self, kind: PotFilterItemKind) -> bool {
        matches!(self.get_filter(kind), Some(nks::FilterItemId(Some(_))))
    }

    pub fn set_filter(
        &mut self,
        kind: PotFilterItemKind,
        id: OptFilter,
        shared_self: SharedRuntimePotUnit,
    ) {
        *self.runtime_state.get_filter_mut(kind) = id;
        self.sender
            .send_complaining(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::FilterItemChanged { kind, filter: id },
            ));
        self.rebuild_collections(shared_self);
    }

    pub fn rebuild_collections(&mut self, shared_self: SharedRuntimePotUnit) {
        let runtime_state = self.runtime_state.clone();
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
            let build_outcome = with_preset_db(|db| db.build_collections(&runtime_state))??;
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

    fn notify_build_outcome_ready(&mut self, build_outcome: BuildOutcome) {
        self.runtime_state.filter_settings = build_outcome.filter_settings;
        self.collections = build_outcome.collections;
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
    // TODO-high CONTINUE Distinguish <Any> and <None> in persistence
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
            persistent_id: "".to_string(),
            id: nks::FilterItemId(None),
            parent_name: None,
            name: Some("<None>".to_string()),
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
    destination: &PresetLoadDestination,
) -> Result<LoadPresetOutcome, &'static str> {
    let nks_file = NksFile::load(&preset.file_name)?;
    let nks_content = nks_file.content()?;
    let fx = make_sure_fx_has_correct_type(nks_content.plugin_id, destination)?;
    fx.set_vst_chunk(nks_content.vst_chunk)?;
    let outcome = LoadPresetOutcome {
        fx,
        current_preset: CurrentPreset::with_parameters(preset.clone(), nks_content.macro_params),
    };
    Ok(outcome)
}

fn load_audio_preset(
    preset: &Preset,
    destination: &PresetLoadDestination,
) -> Result<LoadPresetOutcome, &'static str> {
    const RS5K_VST_ID: u32 = 1920167789;
    let plugin_id = PluginId::Vst2 {
        vst_magic_number: RS5K_VST_ID,
    };
    let fx = make_sure_fx_has_correct_type(plugin_id, destination)?;
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
    let outcome = LoadPresetOutcome {
        fx,
        current_preset: CurrentPreset::without_parameters(preset.clone()),
    };
    Ok(outcome)
}

struct LoadPresetOutcome {
    fx: Fx,
    current_preset: CurrentPreset,
}

fn make_sure_fx_has_correct_type(
    plugin_id: PluginId,
    destination: &PresetLoadDestination,
) -> Result<Fx, &'static str> {
    match destination.resolve() {
        None => insert_fx_by_plugin_id(plugin_id, destination),
        Some(fx) => {
            let fx_info = fx.info()?;
            if fx_info.id == plugin_id.formatted_for_reaper() {
                return Ok(fx);
            }
            // We don't have the right plug-in type. Remove FX and insert correct one.
            destination.chain.remove_fx(&fx)?;
            insert_fx_by_plugin_id(plugin_id, destination)
        }
    }
}

fn insert_fx_by_plugin_id(
    plugin_id: PluginId,
    destination: &PresetLoadDestination,
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

pub struct PresetLoadDestination {
    pub chain: FxChain,
    pub fx_index: u32,
}

impl PresetLoadDestination {
    pub fn first_fx_on_selected_track() -> Result<Self, &'static str> {
        let selected_track = Reaper::get()
            .current_project()
            .first_selected_track(MasterTrackBehavior::IncludeMasterTrack)
            .ok_or("no track selected")?;
        let dest = Self {
            chain: selected_track.normal_fx_chain(),
            fx_index: 0,
        };
        Ok(dest)
    }

    pub fn resolve(&self) -> Option<Fx> {
        self.chain.fx_by_index(self.fx_index)
    }
}

impl Display for PresetLoadDestination {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match self.chain.context() {
            FxChainContext::Monitoring => {
                write!(f, "Monitoring FX chain")?;
            }
            FxChainContext::Track { track, is_input_fx } => {
                let chain_name = if *is_input_fx {
                    "Normal chain"
                } else {
                    "Input chain"
                };
                write!(f, "Track \"{}\" / {chain_name}", track.name().unwrap())?;
            }
            FxChainContext::Take(_) => {
                panic!("take FX chain not yet supported");
            }
        }
        write!(f, " / FX #{}", self.fx_index + 1)?;
        Ok(())
    }
}
