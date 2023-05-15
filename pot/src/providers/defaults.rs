use crate::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::{
    create_plugin_factory_preset, FilterInput, InnerBuildInput, InnerPresetId,
    PersistentDatabaseId, PersistentInnerPresetId, PersistentPresetId, PluginId, Preset,
};
use std::borrow::Cow;

use crate::plugins::PluginCommon;
use either::Either;
use enumset::{enum_set, EnumSet};
use itertools::Itertools;
use realearn_api::persistence::PotFilterKind;
use std::error::Error;
use std::iter;

pub struct DefaultsDatabase {
    persistent_id: PersistentDatabaseId,
    plugins: Vec<PluginCommon>,
}

impl Default for DefaultsDatabase {
    fn default() -> Self {
        Self {
            persistent_id: PersistentDatabaseId::new("fx-defaults".to_string()),
            plugins: vec![],
        }
    }
}

impl DefaultsDatabase {
    pub fn open() -> Self {
        Default::default()
    }

    fn query_presets_internal<'a>(
        &'a self,
        filter_input: &'a FilterInput,
    ) -> impl Iterator<Item = (usize, &PluginCommon)> + 'a {
        let matches = !filter_input.filters.wants_user_presets_only();
        if !matches {
            return Either::Left(iter::empty());
        }
        let iter = self.plugins.iter().enumerate().filter(|(i, p)| {
            let id = InnerPresetId(*i as _);
            filter_input.everything_matches(Some(&p.core), id)
        });
        Either::Right(iter)
    }
}

impl Database for DefaultsDatabase {
    fn persistent_id(&self) -> &PersistentDatabaseId {
        &self.persistent_id
    }

    fn name(&self) -> Cow<str> {
        "FX defaults".into()
    }

    fn description(&self) -> Cow<str> {
        "Default factory presets for all of your plug-ins".into()
    }

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!(PotFilterKind::Bank)
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        // We clone the plug-in list so we can create own own order and maintain stable IDs.
        self.plugins = ctx.plugin_db.plugins().map(|p| p.common.clone()).collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
        _: EnumSet<PotFilterKind>,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut new_filters = *input.filter_input.filters;
        new_filters.clear_this_and_dependent_filters(PotFilterKind::Bank);
        let product_items = self
            .query_presets_internal(&input.filter_input.with_filters(&new_filters))
            .map(|(_, plugin)| plugin.core.product_id)
            .unique()
            .map(InnerFilterItem::Product)
            .collect();
        let mut collections = InnerFilterItemCollections::empty();
        collections.set(PotFilterKind::Bank, product_items);
        Ok(collections)
    }

    fn query_presets(
        &self,
        _: &ProviderContext,
        input: InnerBuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        if !input.search_evaluator.matches(PRESET_NAME) {
            return Ok(vec![]);
        }
        let preset_ids = self
            .query_presets_internal(&input.filter_input)
            .map(|(i, _)| SortablePresetId::new(i as _, PRESET_NAME.to_string()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, _: &ProviderContext, preset_id: InnerPresetId) -> Option<Preset> {
        let plugin = self.plugins.get(preset_id.0 as usize)?;
        let persistent_id = PersistentPresetId::new(
            self.persistent_id.clone(),
            create_persistent_inner_id(&plugin.core.id),
        );
        let preset = create_plugin_factory_preset(plugin, persistent_id, PRESET_NAME.to_string());
        Some(preset)
    }
}

const PRESET_NAME: &str = "<Default>";

/// Example: `vst2|1967946098`
fn create_persistent_inner_id(plugin_id: &PluginId) -> PersistentInnerPresetId {
    let id = plugin_id.to_string();
    PersistentInnerPresetId::new(id)
}
