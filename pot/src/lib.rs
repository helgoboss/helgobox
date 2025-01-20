#![allow(clippy::single_match)]

//! "Pot" is intended to be an abstraction over different preset databases. At the moment it only
//! supports Komplete/NKS. As soon as other database backends are supported, we need to add a few
//! abstractions. Care should be taken to not persist anything that's very specific to a particular
//! database backend. Or at least that existing persistent state can easily migrated to a future
//! state that has support for multiple database backends.

use base::{blocking_lock, blocking_lock_arc, blocking_write_lock, hash_util, SoundPlayer};

use enumset::EnumSet;
use helgobox_api::persistence::PotFilterKind;
use reaper_high::{Chunk, Fx, FxChain, GroupingBehavior, Project, Reaper, Track};
use reaper_medium::{
    reaper_str, FxPresetRef, GangBehavior, InputMonitoringMode, InsertMediaMode,
    MasterTrackBehavior, ParamId, ReaperNormalizedFxParamValue, ReaperVolumeValue, RecordingInput,
};
use std::borrow::Cow;
use std::cell::{Ref, RefMut};
use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::ops::Range;

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use chrono::NaiveDateTime;
use itertools::Itertools;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use wildmatch::WildMatch;

mod api;
pub use api::*;
mod nks;
mod pot_database;
use crate::providers::komplete::NksFile;
pub use pot_database::*;

mod plugin_id;
mod plugins;
use crate::plugins::{PluginCommon, PluginCore};
use crate::provider_database::{DatabaseId, FIL_IS_AVAILABLE_FALSE, FIL_IS_AVAILABLE_TRUE};
pub use plugin_id::*;

mod provider_database;
pub mod providers;
mod worker;
pub use worker::*;
mod escape_catcher;
pub mod preset_crawler;
pub mod preview_recorder;
use crate::preset_crawler::get_shim_file_path;
use crate::preview_recorder::get_preview_file_path_from_hash;
use base::hash_util::{NonCryptoHashSet, NonCryptoIndexSet, PersistentHash};
pub use escape_catcher::*;

// - We have a global list of databases
// - A pot unit doesn't own those databases but it will access them.
// - That means for sure that this list of databases is *shared*.
// - Those databases can and will contain mutable state. Most queries are read-only, but there
//   will be something like rescan() that modifies its state. And in future maybe also write access.
// - That means we need to have a Mutex or RWLock.
// - Question is if we need this RWLock around each database or around the complete list.
// - Having it around the list will probably be necessary in future in order to add more databases
//   at runtime.
// - But having it (maybe additionally in future) around each database makes it possible to
//   query multiple databases in parallel, also from different pot units.

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
        integration: BoxedPotIntegration,
    ) -> Result<SharedRuntimePotUnit, &'static str> {
        match self {
            PotUnit::Unloaded {
                state,
                previous_load_error,
            } => {
                if !previous_load_error.is_empty() {
                    return Err(previous_load_error);
                }
                match RuntimePotUnit::load(state, integration) {
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
            PotUnit::Loaded(u) => {
                blocking_lock(u, "PotUnit from persistence_state").persistent_state()
            }
        }
    }
}

/// This trait is not very good in terms of signatures. It's just a minimum-effort solution to get
/// inversion of control (in order to not let Pot depend on ReaLearn's main code).
pub trait PotIntegration {
    // TODO-high-pot This will probably look different as soon as we seriously implement favorites.
    fn favorites(&self) -> &RwLock<PotFavorites>;
    fn set_current_fx_preset(&self, fx: Fx, preset: CurrentPreset);
    fn exclude_list(&self) -> Ref<PotFilterExcludes>;
    fn exclude_list_mut(&self) -> RefMut<PotFilterExcludes>;
    fn notify_preset_changed(&self, id: Option<PresetId>);
    fn notify_filter_changed(&self, kind: PotFilterKind, filter: OptFilter);
    fn notify_indexes_rebuilt(&self);
    /// Returns an FX instance which must not be removed (e.g. in the process of loading a preset).
    ///
    /// This should be the FX holding the ReaLearn instance which controls the pot.
    fn protected_fx(&self) -> &Fx;
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct RuntimePotUnit {
    pub runtime_state: RuntimeState,
    pub filter_item_collections: FilterItemCollections,
    pub supported_filter_kinds: EnumSet<PotFilterKind>,
    pub preset_collection: PresetCollection,
    pub wasted_runs: u32,
    pub wasted_duration: Duration,
    pub stats: Stats,
    build_counter: u64,
    sound_player: SoundPlayer,
    preview_volume: ReaperVolumeValue,
    pub default_load_preset_window_behavior: LoadPresetWindowBehavior,
    pub destination_descriptor: DestinationDescriptor,
    pub name_track_after_preset: bool,
    show_excluded_filter_items: bool,
    running_background_task: Option<RunningBackgroundTask>,
    #[derivative(Debug = "ignore")]
    integration: BoxedPotIntegration,
}

#[derive(Debug)]
struct RunningBackgroundTask {
    start_time: Instant,
    change_hint: ChangeHint,
}

impl RunningBackgroundTask {
    pub fn new(change_hint: ChangeHint) -> Self {
        Self {
            start_time: Instant::now(),
            change_hint,
        }
    }
}

pub type BoxedPotIntegration = Box<dyn PotIntegration + Send>;

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
    pub refresh_duration: Duration,
    pub filter_query_duration: Duration,
    pub preset_query_duration: Duration,
    pub preview_filter_duration: Duration,
    pub sort_duration: Duration,
    pub index_duration: Duration,
}

impl Stats {
    pub fn total_query_duration(&self) -> Duration {
        self.refresh_duration
            + self.filter_query_duration
            + self.preset_query_duration
            + self.preview_filter_duration
            + self.sort_duration
            + self.index_duration
    }
}

#[derive(Clone)]
pub struct BuildInput {
    pub filters: Filters,
    pub filter_excludes: PotFilterExcludes,
    pub search_evaluator: SearchEvaluator,
}

#[derive(Copy, Clone)]
pub struct InnerBuildInput<'a> {
    pub filter_input: FilterInput<'a>,
    pub search_evaluator: &'a SearchEvaluator,
}

impl<'a> InnerBuildInput<'a> {
    pub fn new(
        input: &'a BuildInput,
        favorites: &'a PotFavorites,
        db_id: DatabaseId,
    ) -> InnerBuildInput<'a> {
        InnerBuildInput {
            filter_input: FilterInput {
                filters: &input.filters,
                excludes: &input.filter_excludes,
                db_favorites: favorites.db_favorites(db_id),
            },
            search_evaluator: &input.search_evaluator,
        }
    }
}

#[derive(Copy, Clone)]
pub struct FilterInput<'a> {
    pub filters: &'a Filters,
    pub excludes: &'a PotFilterExcludes,
    pub db_favorites: &'a NonCryptoHashSet<InnerPresetId>,
}

impl<'a> FilterInput<'a> {
    /// This is a useful method for databases that filter in-memory and map their presets to
    /// existing scanned REAPER plug-ins. The following kinds of filters are checked:
    ///
    /// - Availability
    /// - Support (never matches if showing only unsupported presets)
    /// - Product kind
    /// - Product (also makes sure it's not excluded)
    /// - Favorite
    pub fn everything_matches(
        &self,
        plugin: Option<&PluginCore>,
        preset_id: InnerPresetId,
    ) -> bool {
        let availability_matches = || {
            let fil = if plugin.is_some() {
                FIL_IS_AVAILABLE_TRUE
            } else {
                FIL_IS_AVAILABLE_FALSE
            };
            self.filters.matches(PotFilterKind::IsAvailable, fil)
        };
        let support_matches = || {
            self.filters.get(PotFilterKind::IsSupported)
                != Some(FilterItemId(Some(FIL_IS_AVAILABLE_FALSE)))
        };
        let product_kind_matches = || {
            // If we don't have plug-in info, we also don't have product kind info.
            let fil = plugin.and_then(|p| p.product_kind).map(Fil::ProductKind);
            self.filters
                .matches_optional(PotFilterKind::ProductKind, fil)
        };
        let product_matches = || {
            // If we don't have plug-in info, we also don't have product info.
            let fil = plugin.map(|p| Fil::Product(p.product_id));
            self.filters.matches_optional(PotFilterKind::Bank, fil)
        };
        let product_is_included = || {
            // If we don't have plug-in, we *can* be excluded via product ... if <None> is excluded.
            let product_id = plugin.map(|p| p.product_id);
            !self.excludes.contains_product(product_id)
        };
        let favorite_matches = || self.filters.favorite_matches(self.db_favorites, preset_id);
        // Combine
        availability_matches()
            && support_matches()
            && product_kind_matches()
            && product_matches()
            && favorite_matches()
            && product_is_included()
    }

    pub fn with_filters(&self, filters: &'a Filters) -> Self {
        Self {
            filters,
            excludes: self.excludes,
            db_favorites: self.db_favorites,
        }
    }
}

#[derive(Clone, Default)]
pub struct SearchEvaluator {
    processed_search_expression: String,
    options: SearchOptions,
    wild_match: Option<WildMatch>,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub use_wildcards: bool,
    pub search_fields: EnumSet<SearchField>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            use_wildcards: false,
            search_fields: EnumSet::all(),
        }
    }
}

#[derive(Debug, enumset::EnumSetType, strum::AsRefStr, strum::EnumIter)]
pub enum SearchField {
    #[strum(serialize = "Name")]
    PresetName,
    #[strum(serialize = "Plug-in/product")]
    ProductName,
    #[strum(serialize = "Extension")]
    FileExtension,
}

pub trait SearchInput {
    fn preset_name(&self) -> &str;
    fn product_name(&self) -> Option<Cow<str>>;
    fn file_extension(&self) -> Option<&str>;
}

impl SearchEvaluator {
    pub fn new(raw_search_expression: &str, options: SearchOptions) -> Self {
        let processed_search_expression = raw_search_expression.trim().to_lowercase();
        Self {
            wild_match: if options.use_wildcards {
                Some(WildMatch::new(&processed_search_expression))
            } else {
                None
            },
            processed_search_expression,
            options,
        }
    }

    pub fn processed_search_expression(&self) -> &str {
        &self.processed_search_expression
    }

    pub fn options(&self) -> &SearchOptions {
        &self.options
    }

    pub fn use_wildcards(&self) -> bool {
        self.wild_match.is_some()
    }

    pub fn matches(&self, input: impl SearchInput) -> bool {
        if self.processed_search_expression.is_empty() {
            return true;
        }
        self.options
            .search_fields
            .iter()
            .any(|search_field| match search_field {
                SearchField::PresetName => self.matches_internal(input.preset_name()),
                SearchField::ProductName => {
                    if let Some(product_name) = input.product_name() {
                        self.matches_internal(&product_name)
                    } else {
                        false
                    }
                }
                SearchField::FileExtension => {
                    if let Some(ext) = input.file_extension() {
                        self.matches_internal(ext)
                    } else {
                        false
                    }
                }
            })
    }

    fn matches_internal(&self, text: &str) -> bool {
        let lowercase_text = text.to_lowercase();
        match &self.wild_match {
            None => lowercase_text.contains(&self.processed_search_expression),
            Some(wild_match) => wild_match.matches(&lowercase_text),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChangeHint {
    /// Will refresh databases. Potentially expensive operation!
    TotalRefresh,
    Normal,
    Filter(PotFilterKind),
    SearchExpression,
}

impl ChangeHint {
    pub fn affected_kinds(&self) -> EnumSet<PotFilterKind> {
        use ChangeHint::*;
        match self {
            Normal | TotalRefresh => EnumSet::all(),
            SearchExpression => EnumSet::empty(),
            Filter(changed_kind) => changed_kind.dependent_kinds().collect(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeState {
    pub filters: Filters,
    pub search_expression: String,
    pub search_options: SearchOptions,
    preset_id: Option<PresetId>,
}

impl RuntimeState {
    pub fn load(_persistent_state: &PersistentState) -> Result<Self, &'static str> {
        // with_preset_db(|db| {
        //     let filter_exclude_list = BackboneState::get().pot_filter_exclude_list();
        //     let collections = db.build_filter_items(
        //         &Default::default(),
        //         EnumSet::all().into_iter(),
        //         &filter_exclude_list,
        //     );
        //     let filter_settings = {
        //         let nks = &persistent_state.filter_settings.nks;
        //         let mut filters = Filters::empty();
        //         let mut set_filter = |setting: &Option<String>, kind: PotFilterItemKind| {
        //             let id = setting.as_ref().and_then(|persistent_id| {
        //                 let item = collections
        //                     .get(kind)
        //                     .iter()
        //                     .find(|item| &item.persistent_id == persistent_id)?;
        //                 Some(item.id)
        //             });
        //             filters.set(kind, id);
        //         };
        //         set_filter(&nks.bank, PotFilterItemKind::NksBank);
        //         set_filter(&nks.sub_bank, PotFilterItemKind::NksSubBank);
        //         set_filter(&nks.category, PotFilterItemKind::NksCategory);
        //         set_filter(&nks.sub_category, PotFilterItemKind::NksSubCategory);
        //         set_filter(&nks.mode, PotFilterItemKind::NksMode);
        //         FilterSettings { nks: filters }
        //     };
        //     let preset_id = persistent_state
        //         .preset_id
        //         .as_ref()
        //         .and_then(|persistent_id| db.find_preset_id_by_favorite_id(persistent_id));
        //     Self {
        //         filter_settings,
        //         search_expression: "".to_string(),
        //         use_wildcard_search: false,
        //         preset_id,
        //     }
        // })
        let state = Self {
            filters: Default::default(),
            search_expression: "".to_string(),
            search_options: Default::default(),
            preset_id: None,
        };
        Ok(state)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentState {
    filter_settings: PersistentFilterSettings,
    preset_id: Option<String>,
}

type PresetCollection = NonCryptoIndexSet<PresetId>;

#[derive(Clone, Eq, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PersistentFilterSettings {
    pub nks: nks::PersistentNksFilterSettings,
}

impl RuntimePotUnit {
    pub fn load(
        state: &PersistentState,
        integration: BoxedPotIntegration,
    ) -> Result<SharedRuntimePotUnit, &'static str> {
        let sound_player = SoundPlayer::new();
        let unit = Self {
            runtime_state: RuntimeState::load(state)?,
            filter_item_collections: Default::default(),
            supported_filter_kinds: Default::default(),
            preset_collection: Default::default(),
            wasted_runs: 0,
            wasted_duration: Default::default(),
            stats: Default::default(),
            build_counter: 0,
            preview_volume: sound_player.volume().unwrap_or_default(),
            sound_player,
            destination_descriptor: Default::default(),
            name_track_after_preset: true,
            show_excluded_filter_items: false,
            running_background_task: None,
            integration,
            default_load_preset_window_behavior: Default::default(),
        };
        let shared_unit = Arc::new(Mutex::new(unit));
        blocking_lock_arc(&shared_unit, "PotUnit from load").rebuild_collections(
            shared_unit.clone(),
            ChangeHint::TotalRefresh,
            Debounce::No,
        );
        Ok(shared_unit)
    }

    pub fn preview_volume(&self) -> ReaperVolumeValue {
        self.preview_volume
    }

    pub fn protected_fx(&self) -> &Fx {
        self.integration.protected_fx()
    }

    pub fn set_preview_volume(&mut self, volume: ReaperVolumeValue) {
        self.preview_volume = volume;
        self.sound_player
            .set_volume(volume)
            .expect("changing preview volume value failed");
    }

    pub fn persistent_state(&self) -> PersistentState {
        // let nks_settings = &self.runtime_state.filter_settings.nks;
        // let nks_items = &self.collections.filter_item_collections.nks;
        // let find_id = |kind: PotFilterItemKind| {
        //     nks_settings.get(kind).and_then(|id| {
        //         nks_items
        //             .get(kind)
        //             .iter()
        //             .find(|item| item.id == id)
        //             .map(|item| item.persistent_id.clone())
        //     })
        // };
        // let filter_settings = PersistentFilterSettings {
        //     nks: PersistentNksFilterSettings {
        //         bank: find_id(PotFilterItemKind::NksBank),
        //         sub_bank: find_id(PotFilterItemKind::NksSubBank),
        //         category: find_id(PotFilterItemKind::NksCategory),
        //         sub_category: find_id(PotFilterItemKind::NksSubCategory),
        //         mode: find_id(PotFilterItemKind::NksMode),
        //     },
        // };
        // let preset_id = self.runtime_state.preset_id.and_then(|id| {
        //     with_preset_db(|db| Some(db.find_preset_by_id(id)?.favorite_id))
        //         .ok()
        //         .flatten()
        // });
        // PersistentState {
        //     filter_settings,
        //     preset_id,
        // }
        PersistentState::default()
    }

    pub fn toggle_favorite(&mut self, preset_id: PresetId, shared_self: SharedRuntimePotUnit) {
        let favorites = self.integration.favorites();
        blocking_write_lock(favorites, "favorite toggle").toggle_favorite(preset_id);
        self.rebuild_collections(
            shared_self,
            ChangeHint::Filter(PotFilterKind::IsFavorite),
            Debounce::No,
        );
    }

    pub fn play_preview(&mut self, preset_id: PresetId) -> anyhow::Result<()> {
        let preset = pot_db()
            .find_preset_by_id(preset_id)
            .context("couldn't find preset")?;
        let reaper_resource_dir = Reaper::get().resource_path();
        let preview_file = find_preview_file(&preset, &reaper_resource_dir)
            .context("couldn't find preview file")?;
        self.sound_player.load_file(&preview_file)?;
        self.sound_player.play()?;
        Ok(())
    }

    pub fn stop_preview(&mut self) -> anyhow::Result<()> {
        self.sound_player.stop()
    }

    pub fn preset_and_id(&self) -> Option<(PresetId, PotPreset)> {
        let preset_id = self.preset_id()?;
        let preset = pot_db().find_preset_by_id(preset_id)?;
        Some((preset_id, preset))
    }

    pub fn load_preset(
        &mut self,
        preset: &PotPreset,
        options: LoadPresetOptions,
    ) -> Result<(), LoadPresetError> {
        let build_destination = |pot_unit: &mut Self| {
            let dest = match pot_unit.resolve_destination()? {
                DestinationInstruction::Existing(d) => d,
                DestinationInstruction::AddTrack => {
                    let track = Reaper::get().current_project().add_track()?;
                    track.set_recording_input(Some(RecordingInput::Midi {
                        device_id: None,
                        channel: None,
                    }));
                    track.arm(
                        false,
                        GangBehavior::DenyGang,
                        GroupingBehavior::PreventGrouping,
                    );
                    track.set_input_monitoring_mode(
                        InputMonitoringMode::Normal,
                        GangBehavior::DenyGang,
                        GroupingBehavior::PreventGrouping,
                    );
                    // Reset FX back to first one for UI and next preset load.
                    pot_unit.destination_descriptor.fx_index = 0;
                    track.select_exclusively();
                    Destination {
                        chain: track.normal_fx_chain(),
                        fx_index: 0,
                    }
                }
            };
            Ok(dest)
        };
        let fx = self.load_preset_at(preset, options, &build_destination)?;
        if self.name_track_after_preset {
            if let Some(track) = fx.track() {
                track.set_name(preset.name());
            }
        }
        Ok(())
    }

    pub fn resolve_destination(&self) -> Result<DestinationInstruction, &'static str> {
        self.destination_descriptor.resolve_destination()
    }

    pub fn load_preset_at(
        &mut self,
        preset: &PotPreset,
        options: LoadPresetOptions,
        build_destination: &impl Fn(&mut RuntimePotUnit) -> Result<Destination, &'static str>,
    ) -> Result<Fx, LoadPresetError> {
        let window_behavior = options
            .window_behavior_override
            .unwrap_or(self.default_load_preset_window_behavior);
        match self.load_preset_at_internal(
            preset,
            window_behavior,
            options.audio_sample_behavior,
            build_destination,
        ) {
            Ok(fx) => Ok(fx),
            Err(LoadPresetError::UnsupportedPresetFormat { file_extension, .. }) => {
                // Unsupported format. But maybe we have a shim file?
                let reaper_resource_dir = Reaper::get().resource_path();
                let shim_file_path =
                    get_shim_file_path(&reaper_resource_dir, &preset.common.persistent_id);
                if !shim_file_path.exists() {
                    // Give up
                    return Err(LoadPresetError::UnsupportedPresetFormat {
                        file_extension,
                        is_shim_preset: false,
                    });
                }
                let protected_fx = self.protected_fx().clone();
                let outcome = self.load_file_based_preset(
                    &shim_file_path,
                    build_destination,
                    window_behavior,
                    options.audio_sample_behavior,
                    true,
                    &protected_fx,
                )?;
                Ok(self.process_preset_load_outcome(preset, outcome))
            }
            e => e,
        }
    }

    fn load_file_based_preset(
        &mut self,
        preset_file: &Utf8Path,
        build_destination: impl Fn(&mut RuntimePotUnit) -> Result<Destination, &'static str>,
        window_behavior: LoadPresetWindowBehavior,
        audio_sample_behavior: LoadAudioSampleBehavior,
        is_shim_preset: bool,
        protected_fx: &Fx,
    ) -> Result<LoadPresetOutcome, LoadPresetError> {
        let ext = preset_file
            .extension()
            .ok_or(LoadPresetError::UnsupportedPresetFormat {
                file_extension: "".to_string(),
                is_shim_preset,
            })?;
        let outcome = match ext.to_lowercase().as_str() {
            _ if is_audio_file_extension(ext) => {
                let dest = build_destination(self)?;
                load_audio_preset(
                    preset_file,
                    &dest,
                    window_behavior,
                    audio_sample_behavior,
                    protected_fx,
                )?
            }
            "nksf" | "nksfx" => {
                let dest = build_destination(self)?;
                load_nks_preset(
                    preset_file.as_std_path(),
                    &dest,
                    window_behavior,
                    protected_fx,
                )?
            }
            "rfxchain" => {
                let dest = build_destination(self)?;
                load_rfx_chain_preset_using_chunks(
                    preset_file.as_std_path(),
                    &dest,
                    window_behavior,
                    protected_fx,
                )?
            }
            "rtracktemplate" => {
                let dest = build_destination(self)?;
                load_track_template_preset(
                    preset_file.as_std_path(),
                    &dest,
                    window_behavior,
                    protected_fx,
                )?
            }
            x => {
                return Err(LoadPresetError::UnsupportedPresetFormat {
                    file_extension: x.to_string(),
                    is_shim_preset,
                });
            }
        };
        Ok(outcome)
    }

    /// Doesn't try to load a shim preset yet.
    fn load_preset_at_internal(
        &mut self,
        preset: &PotPreset,
        window_behavior: LoadPresetWindowBehavior,
        audio_sample_behavior: LoadAudioSampleBehavior,
        build_destination: &impl Fn(&mut RuntimePotUnit) -> Result<Destination, &'static str>,
    ) -> Result<Fx, LoadPresetError> {
        let _ = self.sound_player.stop();
        let protected_fx = self.protected_fx().clone();
        let outcome = match &preset.kind {
            PotPresetKind::FileBased(k) => self.load_file_based_preset(
                &k.path,
                build_destination,
                window_behavior,
                audio_sample_behavior,
                false,
                &protected_fx,
            )?,
            PotPresetKind::ProjectBased(k) => load_project_based_rfx_chain_preset(
                self,
                build_destination,
                &k.path_to_rpp,
                k.fx_chain_range.clone(),
                window_behavior,
                &protected_fx,
            )?,
            PotPresetKind::Internal(k) => {
                if let Some(plugin_id) = k.plugin_id {
                    let dest = build_destination(self)?;
                    load_internal_preset(
                        plugin_id,
                        preset.name(),
                        &dest,
                        window_behavior,
                        &protected_fx,
                    )
                    .map_err(LoadPresetError::Other)?
                } else {
                    return Err(LoadPresetError::Other(
                        "Plug-in for internal preset not found".into(),
                    ));
                }
            }
            PotPresetKind::DefaultFactory(plugin_id) => {
                let dest = build_destination(self)?;
                load_default_factory_preset(*plugin_id, &dest, window_behavior, &protected_fx)
                    .map_err(LoadPresetError::Other)?
            }
        };
        let fx = self.process_preset_load_outcome(preset, outcome);
        Ok(fx)
    }

    fn process_preset_load_outcome(&self, preset: &PotPreset, outcome: LoadPresetOutcome) -> Fx {
        let current_preset = CurrentPreset {
            preset: preset.clone(),
            macro_param_banks: outcome.banks,
        };
        self.integration
            .set_current_fx_preset(outcome.fx.clone(), current_preset);
        outcome.fx
    }

    pub fn state(&self) -> &RuntimeState {
        &self.runtime_state
    }

    pub fn preset_id(&self) -> Option<PresetId> {
        self.runtime_state.preset_id
    }

    pub fn find_currently_selected_preset(&self) -> Option<PotPreset> {
        let preset_id = self.runtime_state.preset_id?;
        pot_db().find_preset_by_id(preset_id)
    }

    pub fn set_preset_id(&mut self, id: Option<PresetId>) {
        self.runtime_state.preset_id = id;
        self.integration.notify_preset_changed(id);
    }

    pub fn filters(&self) -> &Filters {
        &self.runtime_state.filters
    }

    pub fn supports_filter_kind(&self, kind: PotFilterKind) -> bool {
        self.supported_filter_kinds.contains(kind)
    }

    pub fn get_filter(&self, kind: PotFilterKind) -> OptFilter {
        self.runtime_state.filters.get(kind)
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
        self.clear_invalid_filters();
        self.rebuild_collections(shared_self, ChangeHint::Normal, Debounce::No);
    }

    pub fn include_filter_item(
        &mut self,
        kind: PotFilterKind,
        id: FilterItemId,
        include: bool,
        shared_self: SharedRuntimePotUnit,
    ) {
        {
            let mut list = self.integration.exclude_list_mut();
            if include {
                list.remove(kind, id);
            } else {
                list.add(kind, id);
            }
        }
        self.clear_invalid_filters();
        self.rebuild_collections(shared_self, ChangeHint::Normal, Debounce::No);
    }

    fn clear_invalid_filters(&mut self) {
        if !self.show_excluded_filter_items {
            let excludes = self.integration.exclude_list();
            self.runtime_state.filters.clear_excluded_ones(&excludes);
        }
    }

    pub fn set_filter(
        &mut self,
        kind: PotFilterKind,
        id: OptFilter,
        shared_self: SharedRuntimePotUnit,
        debounce: Debounce,
    ) {
        self.runtime_state.filters.set(kind, id);
        self.integration.notify_filter_changed(kind, id);
        self.rebuild_collections(shared_self, ChangeHint::Filter(kind), debounce);
    }

    pub fn refresh_pot(&mut self, shared_self: SharedRuntimePotUnit) {
        self.rebuild_collections(shared_self, ChangeHint::TotalRefresh, Debounce::No);
    }

    pub fn create_build_input(&self) -> BuildInput {
        BuildInput {
            filters: self.runtime_state.filters,
            search_evaluator: SearchEvaluator::new(
                &self.runtime_state.search_expression,
                self.runtime_state.search_options.clone(),
            ),
            filter_excludes: if self.show_excluded_filter_items {
                PotFilterExcludes::default()
            } else {
                self.integration.exclude_list().clone()
            },
        }
    }

    pub fn rebuild_collections(
        &mut self,
        shared_self: SharedRuntimePotUnit,
        change_hint: ChangeHint,
        debounce: Debounce,
    ) {
        if self.is_refreshing() {
            // If we are refreshing, we should let it finish first, otherwise the results will
            // be wrong.
            return;
        }
        let build_input = self.create_build_input();
        self.build_counter += 1;
        let build_number = self.build_counter;
        let affected_kinds = change_hint.affected_kinds();
        // Spawn new async task (don't block GUI thread, might take longer)
        spawn_in_pot_worker(async move {
            // Debounce (cheap)
            // If we don't do this, the wasted runs will dramatically increase when quickly changing
            // filters while last query still running.
            {
                if debounce == Debounce::Yes {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                let mut pot_unit =
                    blocking_lock_arc(&shared_self, "PotUnit from rebuild_collections 1");
                if pot_unit.build_counter == build_number {
                    // Okay, no new build was requested in the meantime. Start spinner.
                    pot_unit.running_background_task =
                        Some(RunningBackgroundTask::new(change_hint));
                } else {
                    // Oh, another build was requested already. Not worth to continue, the result
                    // will be discarded anyway.
                    return Ok(());
                }
            }
            // Refresh if desired (very expensive)
            let refresh_start = Instant::now();
            if change_hint == ChangeHint::TotalRefresh {
                pot_db().refresh();
            }
            let refresh_duration = refresh_start.elapsed();
            // Build (expensive)
            let build_output = pot_db().build_collections(build_input, affected_kinds);
            // Set result (cheap)
            // Only set result if no new build has been requested in the meantime.
            // Prevents flickering and increment/decrement issues.
            let mut pot_unit =
                blocking_lock_arc(&shared_self, "PotUnit from rebuild_collections 2");
            if pot_unit.build_counter != build_number {
                pot_unit.wasted_duration += build_output.stats.total_query_duration();
                pot_unit.wasted_runs += 1;
                return Ok(());
            }
            pot_unit.notify_build_outcome_ready(build_output, affected_kinds, refresh_duration);
            Ok(())
        });
    }

    pub fn is_refreshing(&self) -> bool {
        self.running_background_task
            .as_ref()
            .map(|t| t.change_hint == ChangeHint::TotalRefresh)
            .unwrap_or(false)
    }

    pub fn background_task_elapsed(&self) -> Option<Duration> {
        Some(self.running_background_task.as_ref()?.start_time.elapsed())
    }

    fn notify_build_outcome_ready(
        &mut self,
        build_output: BuildOutput,
        affected_kinds: EnumSet<PotFilterKind>,
        refresh_duration: Duration,
    ) {
        self.running_background_task = None;
        self.supported_filter_kinds = build_output.supported_filter_kinds;
        self.preset_collection = build_output.preset_collection;
        for (kind, collection) in build_output.filter_item_collections.into_iter() {
            if affected_kinds.contains(kind) {
                self.filter_item_collections.set(kind, collection);
            }
        }
        self.runtime_state
            .filters
            .clear_if_not_available_anymore(affected_kinds, &self.filter_item_collections);
        self.stats = build_output.stats;
        self.stats.refresh_duration = refresh_duration;
        self.integration.notify_indexes_rebuilt();
    }

    pub fn count_filter_items(&self, kind: PotFilterKind) -> u32 {
        self.filter_item_collections.get(kind).len() as u32
    }

    pub fn preset_count(&self) -> u32 {
        self.preset_collection.len() as u32
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
        let index = self.preset_collection.get_index_of(&id)?;
        Some(index as _)
    }

    pub fn find_preset_id_at_index(&self, index: u32) -> Option<PresetId> {
        self.preset_collection.get_index(index as _).copied()
    }

    pub fn find_filter_item_id_at_index(
        &self,
        kind: PotFilterKind,
        index: u32,
    ) -> Option<FilterItemId> {
        Some(self.find_filter_item_at_index(kind, index)?.id)
    }

    pub fn find_filter_item_at_index(
        &self,
        kind: PotFilterKind,
        index: u32,
    ) -> Option<&FilterItem> {
        self.filter_item_collections.get(kind).get(index as usize)
    }

    pub fn find_index_of_filter_item(&self, kind: PotFilterKind, id: FilterItemId) -> Option<u32> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.0)
    }

    pub fn find_filter_item_by_id(
        &self,
        kind: PotFilterKind,
        id: FilterItemId,
    ) -> Option<&FilterItem> {
        Some(self.find_filter_item_and_index_by_id(kind, id)?.1)
    }

    fn find_filter_item_and_index_by_id(
        &self,
        kind: PotFilterKind,
        id: FilterItemId,
    ) -> Option<(u32, &FilterItem)> {
        fn find(items: &[FilterItem], id: FilterItemId) -> Option<(u32, &FilterItem)> {
            let (i, item) = items.iter().enumerate().find(|(_, item)| item.id == id)?;
            Some((i as u32, item))
        }
        find(self.filter_item_collections.get(kind), id)
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
    pub icon: Option<char>,
    pub more_info: Option<String>,
}

impl FilterItem {
    pub fn none() -> Self {
        Self {
            // TODO-high-pot Persistence
            persistent_id: "".to_string(),
            id: FilterItemId(None),
            parent_name: None,
            name: Some("<None>".to_string()),
            icon: None,
            more_info: None,
        }
    }

    pub fn simple(fil: Fil, name: &str, icon: char, more: &str) -> Self {
        Self {
            // TODO-high-pot Persistence
            persistent_id: "".to_string(),
            id: FilterItemId(Some(fil)),
            parent_name: None,
            name: Some(name.to_string()),
            icon: Some(icon),
            more_info: if more.is_empty() {
                None
            } else {
                Some(more.to_string())
            },
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

    pub fn sort_name(&self) -> &str {
        match &self.name {
            None => match &self.parent_name {
                None => "",
                Some(n) => n,
            },
            Some(n) => n,
        }
    }
}

#[derive(Debug)]
pub struct PresetWithId {
    pub id: PresetId,
    pub preset: PotPreset,
}

impl PresetWithId {
    pub fn new(id: PresetId, preset: PotPreset) -> Self {
        Self { id, preset }
    }
}

impl AsRef<PotPreset> for PresetWithId {
    fn as_ref(&self) -> &PotPreset {
        &self.preset
    }
}

/// Contains meta-data about a preset.
///
/// Although the preset usually doesn't contain the preset chunk (the content to actually load the
/// preset), it's still a bit heavy-weight, containing much meta-data and stuff. So we should
/// not have a list of all presets in memory, for example.
#[derive(Clone, Debug)]
pub struct PotPreset {
    pub common: PotPresetCommon,
    pub kind: PotPresetKind,
}

#[derive(Clone, Debug, Default)]
pub struct PotPresetMetaData {
    pub author: Option<String>,
    pub vendor: Option<String>,
    pub comment: Option<String>,
    pub file_size_in_bytes: Option<u64>,
    pub modification_date: Option<NaiveDateTime>,
}

impl PotPreset {
    pub fn new(common: PotPresetCommon, kind: PotPresetKind) -> Self {
        Self { common, kind }
    }

    pub fn name(&self) -> &str {
        &self.common.name
    }
}

#[derive(Clone, Debug)]
pub struct PotPresetCommon {
    /// ID of the preset that survives restarts and even rescans.
    ///
    /// It might be tempting to let the persistent ID be based on the contents of the preset
    /// but that would be wrong. It also needs to survive modifications of the preset!
    pub persistent_id: PersistentPresetId,
    pub name: String,
    /// Used e.g. for Project database, will contain the project name.
    pub context_name: Option<String>,
    pub plugin_ids: Vec<PluginId>,
    pub product_ids: Vec<ProductId>,
    /// Meaning depends on the database.
    ///
    /// - In case of Komplete, this sometimes corresponds to the name of a plug-in (albeit not
    ///   necessarily the accurate name), e.g. FM8 as far as REAPER's plug-in scan is concerned.
    ///   At other times it corresponds to an instrument within a plug-in, e.g. in case
    ///   of "Abbey Road 50s Drummer" being an instrument within Kontakt.
    /// - In case of other databases, this usually corresponds to the accurate plug-in name.
    pub product_name: Option<String>,
    /// Optional xxh3 128-bit hash based on the contents of the preset.
    ///
    /// This will be used in the function that determines the Pot preview file location. If this is
    /// `None`, the algorithm will take the persistent database and preset IDs, hash them via
    /// xxh3 128-bit and turn this into a directory path. If `content_hash` is  `Some`, the
    /// algorithm will take this value directly and turn it into a directory path
    /// (without re-hashing).
    ///
    /// It's recommended to set this hash if it's not too slow. It should really be based on the
    /// content of the preset. Changing the content would change the hash and therefore invalidate
    /// the preview, which is what we want.
    ///
    /// It's okay for a provider to switch from not setting this hash to setting it. But not the
    /// other way around! Switching from `None` to `Some` is okay because the preview lookup
    /// mechanism will try to find the preview file based on the persistent IDs if it couldn't find
    /// the file based on the contents.
    ///
    /// Once a provider starts setting this hash, it's a sort of commitment! The provider should
    /// make sure that the hash is provided in future as well and that the same preset contents lead
    /// to the same hash. Otherwise, preview files can't be found anymore and need to be recreated.
    pub content_hash: Option<PersistentHash>,
    /// If the database provides its own preview file, this field should contain its hypothetical
    /// path. It's not necessary or encouraged to already check for its existence of this file. This
    /// will be checked by the consumer.
    pub db_specific_preview_file: Option<Utf8PathBuf>,
    pub is_supported: bool,
    pub is_available: bool,
    pub metadata: PotPresetMetaData,
}

impl PotPresetCommon {
    /// Returns the hash that should be used to identify the location of the custom preview file.
    pub fn content_or_id_hash(&self) -> PersistentHash {
        self.content_hash.unwrap_or_else(|| {
            // If there's none, we take the persistent ID.
            hash_util::calculate_persistent_non_crypto_hash_one_shot(
                self.persistent_id.to_string().as_bytes(),
            )
        })
    }
}

#[derive(Clone, Debug)]
pub enum PotPresetKind {
    FileBased(FiledBasedPotPresetKind),
    ProjectBased(ProjectBasedPotPresetKind),
    Internal(InternalPotPresetKind),
    DefaultFactory(PluginId),
}

impl PotPresetKind {
    pub fn file_extension(&self) -> Option<&str> {
        match self {
            PotPresetKind::FileBased(k) => Some(&k.file_ext),
            PotPresetKind::Internal(_) => None,
            PotPresetKind::DefaultFactory(_) => None,
            PotPresetKind::ProjectBased(_) => None,
        }
    }
}

/// The kind of preset that's saved in a separate file.
#[derive(Clone, Debug)]
pub struct FiledBasedPotPresetKind {
    pub path: Utf8PathBuf,
    pub file_ext: String,
}

/// The kind of preset that's buried in a project file.
#[derive(Clone, Debug)]
pub struct ProjectBasedPotPresetKind {
    pub path_to_rpp: PathBuf,
    pub fx_chain_range: Range<usize>,
}

/// The kind of preset that's saved together with the plug-in in REAPER's plug-in GUI, not exported
/// to a separate file.
#[derive(Clone, Debug)]
pub struct InternalPotPresetKind {
    pub plugin_id: Option<PluginId>,
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

fn load_nks_preset(
    path: &Path,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    let nks_file = NksFile::load(path)?;
    let mut nks_content = nks_file.content()?;
    load_preset_single_fx(
        nks_content.plugin_id,
        destination,
        window_behavior,
        protected_fx,
        |fx| {
            fx.set_vst_chunk(nks_content.vst_chunk)?;
            resolve_macro_param_ids(&mut nks_content.macro_param_banks, fx);
            let outcome = InternalLoadPresetOutcome {
                banks: nks_content.macro_param_banks,
            };
            Ok(outcome)
        },
    )
}

fn resolve_macro_param_ids(banks: &mut [MacroParamBank], fx: &Fx) {
    for bank in banks {
        for param in bank.params_mut() {
            if let Some(fx_param) = &mut param.fx_param {
                fx_param.resolved_param_index =
                    resolve_macro_param_id_to_index(fx_param.param_id, fx);
            }
        }
    }
}

pub fn resolve_macro_param_id_to_index(param_id: PotFxParamId, fx: &Fx) -> Option<u32> {
    match param_id {
        PotFxParamId::Index(i) => Some(i),
        PotFxParamId::Id(id) => {
            let param = fx.parameter_by_id(ParamId::custom(format!(":{id}")))?;
            Some(param.index())
        }
    }
}

/// Loads an RfxChain file using the "add fx" API.
///
/// Pros:
/// - Easy API, no dealing with chunks
///
/// Cons:
/// - Much more disruptive than it needs to be if the plug-in types stay the same
/// - RfxChain file needs to be in the "FXChains" folder
#[allow(dead_code)]
fn load_rfx_chain_preset_using_add_fx(
    path: &Path,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    load_preset_multi_fx(destination, window_behavior, true, protected_fx, || {
        let root_dir = Reaper::get().resource_path().join("FXChains");
        let relative_path = path.strip_prefix(&root_dir)?;
        let relative_path = relative_path.to_string_lossy().to_string();
        let fx = destination
            .chain
            .add_fx_by_original_name(relative_path)
            .ok_or("couldn't load FX chain file")?;
        Ok(fx)
    })
}

/// Loads an RfxChain file using chunk replacement.
///
/// Pros:
/// - No need to remove existing FX (very fast and non-disruptive)
/// - RfxChain file can be at any location
///
/// Cons:
/// - Needs to replace the complete track chunk. Modern REAPER API provides a way to make this
///   safe via dynamically resized buffers but we haven't incorporated this yet.
fn load_rfx_chain_preset_using_chunks(
    path: &Path,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    let rppxml =
        fs::read_to_string(path).map_err(|_| "couldn't read FX chain template as string")?;
    load_rfx_chain_preset_using_chunks_from_string(
        &rppxml,
        destination,
        window_behavior,
        protected_fx,
    )
}

fn load_rfx_chain_preset_using_chunks_from_string(
    rppxml: &str,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    load_preset_multi_fx(destination, window_behavior, false, protected_fx, || {
        // Our chunk stuff assumes we have a chunk without indentation. Time to use proper parsing...
        let rppxml: String = rppxml.lines().map(|l| l.trim()).join("\n");
        let fx_chain_content_chunk = Chunk::new(rppxml);
        let fx_chain_chunk = format!("<FXCHAIN\n{fx_chain_content_chunk}\n>");
        destination.chain.set_chunk(&fx_chain_chunk)?;
        let first_fx = destination
            .chain
            .first_fx()
            .ok_or("couldn't get hold of first FX on chain")?;
        Ok(first_fx)
    })
}

fn load_project_based_rfx_chain_preset(
    pot_unit: &mut RuntimePotUnit,
    build_destination: impl Fn(&mut RuntimePotUnit) -> Result<Destination, &'static str>,
    path_to_rpp: &Path,
    fx_chain_range: Range<usize>,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    let project_rppxml =
        fs::read_to_string(path_to_rpp).map_err(|_| "couldn't read project as string")?;
    let fx_chain_rppxml = &project_rppxml[fx_chain_range];
    let destination = &build_destination(pot_unit)?;
    load_rfx_chain_preset_using_chunks_from_string(
        fx_chain_rppxml,
        destination,
        window_behavior,
        protected_fx,
    )
}

fn load_track_template_preset(
    path: &Path,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    load_preset_multi_fx(destination, window_behavior, false, protected_fx, || {
        let rppxml =
            fs::read_to_string(path).map_err(|_| "couldn't read track template as string")?;
        // Our chunk stuff assumes we have a chunk without indentation. Time to use proper parsing...
        let rppxml: String = rppxml.lines().map(|l| l.trim()).join("\n");
        let track_chunk = Chunk::new(rppxml);
        let fx_chain_region = track_chunk
            .region()
            .find_first_tag_named(0, "FXCHAIN")
            .ok_or("track template doesn't have FX chain")?;
        destination.chain.set_chunk(&fx_chain_region.content())?;
        let first_fx = destination
            .chain
            .first_fx()
            .ok_or("couldn't get hold of first FX on chain")?;
        Ok(first_fx)
    })
}

fn load_internal_preset(
    plugin_id: PluginId,
    preset_name: &str,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    load_preset_single_fx(
        plugin_id,
        destination,
        window_behavior,
        protected_fx,
        |fx| {
            fx.activate_preset_by_name(preset_name)?;
            Ok(Default::default())
        },
    )
}

fn load_default_factory_preset(
    plugin_id: PluginId,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    load_preset_single_fx(
        plugin_id,
        destination,
        window_behavior,
        protected_fx,
        |fx| {
            fx.activate_preset(FxPresetRef::FactoryPreset)?;
            Ok(Default::default())
        },
    )
}

fn load_audio_preset(
    path: &Utf8Path,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    audio_sample_behavior: LoadAudioSampleBehavior,
    protected_fx: &Fx,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    const RS5K_VST_ID: i32 = 1920167789;
    let plugin_id = PluginId::Vst2 {
        vst_magic_number: RS5K_VST_ID,
    };
    load_preset_single_fx(
        plugin_id,
        destination,
        window_behavior,
        protected_fx,
        |fx| {
            // First try it the modern way ...
            if load_media_in_specific_rs5k_modern(fx, path).is_err() {
                // ... and if this didn't work, try it the old-school way.
                // Make sure RS5k has focus
                let window_is_open_now = fx.window_is_open();
                if window_is_open_now {
                    if !fx.window_has_focus() {
                        fx.hide_floating_window()?;
                        fx.show_in_floating_window()?;
                    }
                } else {
                    fx.show_in_floating_window()?;
                }
                // Load into RS5k
                load_media_in_last_focused_rs5k(path)?;
            }
            // Set RS5k options
            let (mode, pitch_for_start_note) =
                if let Some(root_pitch) = audio_sample_behavior.root_pitch {
                    // Value range -80 to 80, makes 161 discrete values.
                    let normalized_value = (root_pitch + 80).max(0) as f64 / 160.0;
                    (
                        reaper_str!("2"),
                        ReaperNormalizedFxParamValue::new(normalized_value),
                    )
                } else {
                    (reaper_str!("1"), ReaperNormalizedFxParamValue::default())
                };
            unsafe {
                let _ = fx.set_named_config_param("MODE", mode.as_c_str().as_ptr() as _);
            }
            let _ = fx
                .parameter_by_index(5)
                .set_reaper_normalized_value(pitch_for_start_note);
            let obey_note_off_val =
                ReaperNormalizedFxParamValue::new(audio_sample_behavior.obey_note_off.into());
            let _ = fx
                .parameter_by_index(11)
                .set_reaper_normalized_value(obey_note_off_val);
            Ok(Default::default())
        },
    )
}

fn load_preset_single_fx(
    plugin_id: PluginId,
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    protected_fx: &Fx,
    f: impl FnOnce(&Fx) -> Result<InternalLoadPresetOutcome, Box<dyn Error>>,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    let existing_fx = destination.resolve();
    let fx_was_open_before = existing_fx
        .as_ref()
        .map(|fx| fx.window_is_open())
        .unwrap_or(false);
    let output = ensure_fx_has_correct_type(plugin_id, destination, existing_fx, protected_fx)?;
    let outcome = f(&output.fx)?;
    window_behavior.open_or_close(&output.fx, fx_was_open_before, output.op)?;
    let outcome = LoadPresetOutcome {
        fx: output.fx,
        banks: outcome.banks,
    };
    Ok(outcome)
}

#[derive(Default)]
struct InternalLoadPresetOutcome {
    banks: Vec<MacroParamBank>,
}

fn load_preset_multi_fx(
    destination: &Destination,
    window_behavior: LoadPresetWindowBehavior,
    remove_existing_fx_manually: bool,
    protected_fx: &Fx,
    f: impl FnOnce() -> Result<Fx, Box<dyn Error>>,
) -> Result<LoadPresetOutcome, Box<dyn Error>> {
    let mut fx_was_open_before = false;
    for fx in destination
        .chain
        .fxs()
        .skip(destination.fx_index as _)
        .rev()
    {
        if fx.window_is_open() {
            fx_was_open_before = true;
        }
        if &fx == protected_fx {
            return Err(CANT_REMOVE_PROTECTED_FX.into());
        }
        if remove_existing_fx_manually {
            destination.chain.remove_fx(&fx)?;
        }
    }
    let fx = f()?;
    for fx in destination.chain.fxs() {
        window_behavior.open_or_close(&fx, fx_was_open_before, FxEnsureOp::Replaced)?;
    }
    let outcome = LoadPresetOutcome { fx, banks: vec![] };
    Ok(outcome)
}

struct LoadPresetOutcome {
    fx: Fx,
    banks: Vec<MacroParamBank>,
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
    protected_fx: &Fx,
) -> Result<FxEnsureOutput, Box<dyn Error>> {
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
            if fx_info.id == plugin_id.content_formatted_for_reaper() {
                // This is the right plug-in type. Leave as is.
                FxEnsureOutput {
                    fx,
                    op: FxEnsureOp::Same,
                }
            } else {
                // We don't have the right plug-in type. Remove FX and insert correct one.
                if &fx == protected_fx {
                    return Err(CANT_REMOVE_PROTECTED_FX.into());
                }
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
) -> Result<Fx, Box<dyn Error>> {
    let kind = plugin_id.kind();
    let name = format!(
        "{}{}{}",
        kind.reaper_add_by_name_prefix_fix(),
        kind.formatted_for_reaper(),
        PluginIdContentInReaperFormat(&plugin_id)
    );
    let fx = destination
        .chain
        .insert_fx_by_name(destination.fx_index, name.as_str())
        .ok_or_else(|| format!("Couldn't add FX via name \"{name}\""))?;
    Ok(fx)
}

fn load_media_in_specific_rs5k_modern(fx: &Fx, path: &Utf8Path) -> Result<(), Box<dyn Error>> {
    let path_c_string = CString::new(path.as_str())?;
    unsafe {
        fx.set_named_config_param("FILE", path_c_string.as_bytes_with_nul().as_ptr() as _)?;
    }
    Ok(())
}

fn load_media_in_last_focused_rs5k(path: &Utf8Path) -> Result<(), &'static str> {
    Reaper::get().medium_reaper().insert_media(
        path,
        InsertMediaMode::CurrentReasamplomatic,
        Default::default(),
    )?;
    Ok(())
}

#[derive(Clone, Debug)]
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
    pub window_behavior_override: Option<LoadPresetWindowBehavior>,
    pub audio_sample_behavior: LoadAudioSampleBehavior,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct LoadAudioSampleBehavior {
    /// If `None`, MIDI notes will be ignored.
    ///
    /// If `Some`:
    /// - Sets RS5k parameter "Mode" (named parameter "MODE") to "Note (Semitone shifted)" (2).
    /// - Sets RS5k parameter "Pitch for start note" (index 5), which has range -80 to +80.
    pub root_pitch: Option<i32>,
    /// Sets RS5k parameter "Obey note-offs" (index 11), which is boolean.
    pub obey_note_off: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, strum::EnumIter, strum::AsRefStr)]
pub enum LoadPresetWindowBehavior {
    #[strum(serialize = "Never show")]
    NeverShow,
    #[strum(serialize = "Show only if previously shown")]
    ShowOnlyIfPreviouslyShown,
    #[strum(serialize = "Show only if previously shown or newly added")]
    #[default]
    ShowOnlyIfPreviouslyShownOrNewlyAdded,
    #[strum(serialize = "Always show")]
    AlwaysShow,
}

impl LoadPresetWindowBehavior {
    pub fn open_or_close(
        &self,
        fx: &Fx,
        was_open_before: bool,
        op: FxEnsureOp,
    ) -> anyhow::Result<()> {
        let now_is_open = fx.window_is_open();
        match self {
            LoadPresetWindowBehavior::NeverShow => {
                if now_is_open {
                    fx.hide_floating_window()?;
                }
            }
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShown => {
                if !was_open_before && now_is_open {
                    fx.hide_floating_window()?;
                } else if was_open_before && !now_is_open {
                    fx.show_in_floating_window()?;
                }
            }
            LoadPresetWindowBehavior::AlwaysShow => {
                if !now_is_open {
                    fx.show_in_floating_window()?;
                }
            }
            LoadPresetWindowBehavior::ShowOnlyIfPreviouslyShownOrNewlyAdded => {
                if op == FxEnsureOp::Added {
                    if !now_is_open {
                        fx.show_in_floating_window()?;
                    }
                } else if !was_open_before && now_is_open {
                    fx.hide_floating_window()?;
                } else if was_open_before && !now_is_open {
                    fx.show_in_floating_window()?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, derive_more::Display)]
pub enum LoadPresetError {
    #[display(fmt = "Unsupported preset format")]
    UnsupportedPresetFormat {
        file_extension: String,
        /// `true` if a shim preset file was found but even its format is not supported.
        /// That would be weird.
        is_shim_preset: bool,
    },
    Other(Box<dyn Error>),
}

impl From<&str> for LoadPresetError {
    fn from(value: &str) -> Self {
        Self::Other(value.into())
    }
}

impl From<Box<dyn Error>> for LoadPresetError {
    fn from(value: Box<dyn Error>) -> Self {
        Self::Other(value)
    }
}

impl Error for LoadPresetError {}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Debounce {
    No,
    Yes,
}

fn is_audio_file_extension(ext: &str) -> bool {
    matches!(ext, "wav" | "aif" | "ogg" | "mp3")
}

pub fn preview_exists(preset: &PotPreset, reaper_resource_dir: &Utf8Path) -> bool {
    find_preview_file(preset, reaper_resource_dir).is_some()
}

/// This looks up the preview file for the given preset, actually checking for the file's
/// existence.
///
/// It also exposes audio file presets as preview files.
///
/// It prefers custom previews over database-specific previews.
pub fn find_preview_file<'a>(
    preset: &'a PotPreset,
    reaper_resource_dir: &Utf8Path,
) -> Option<Cow<'a, Utf8Path>> {
    // If the preset is an audio file and it exists, return that
    if let PotPresetKind::FileBased(kind) = &preset.kind {
        if is_audio_file_extension(&kind.file_ext) {
            return if kind.path.exists() {
                Some(kind.path.as_path().into())
            } else {
                None
            };
        }
    }
    // If a custom preview file exists, return that
    let hash = preset.common.content_or_id_hash();
    let preview_file_path = get_preview_file_path_from_hash(reaper_resource_dir, hash);
    if preview_file_path.exists() {
        return Some(preview_file_path.into());
    }
    // If a database-specific preview file exists, return that
    let db_specific_preview_file = preset.common.db_specific_preview_file.as_ref()?;
    if db_specific_preview_file.exists() {
        Some(Cow::Borrowed(db_specific_preview_file))
    } else {
        None
    }
}

pub fn create_plugin_factory_preset(
    plugin: &PluginCommon,
    persistent_id: PersistentPresetId,
    preset_name: String,
) -> PotPreset {
    PotPreset {
        common: PotPresetCommon {
            persistent_id,
            name: preset_name,
            context_name: None,
            plugin_ids: vec![plugin.core.id],
            product_ids: vec![plugin.core.product_id],
            product_name: Some(plugin.to_string()),
            content_hash: None,
            db_specific_preview_file: None,
            is_supported: true,
            is_available: true,
            metadata: Default::default(),
        },
        kind: PotPresetKind::DefaultFactory(plugin.core.id),
    }
}

const CANT_REMOVE_PROTECTED_FX: &str = "Can't replace ReaLearn itself. Either set \"Load into\" correctly or put ReaLearn on the monitoring FX chain!";
