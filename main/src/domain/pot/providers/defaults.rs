use crate::domain::pot::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
    FIL_IS_FAVORITE_TRUE, FIL_IS_USER_PRESET_TRUE,
};
use crate::domain::pot::{
    BuildInput, FilterItemId, Filters, InnerPresetId, Preset, PresetCommon, PresetKind,
};

use crate::domain::pot::plugins::PluginCommon;
use either::Either;
use itertools::Itertools;
use realearn_api::persistence::PotFilterItemKind;
use std::error::Error;
use std::iter;
use std::path::PathBuf;

#[derive(Default)]
pub struct DefaultsDatabase {
    plugins: Vec<PluginCommon>,
}

impl DefaultsDatabase {
    pub fn open() -> Self {
        Default::default()
    }

    fn query_presets_internal<'a>(
        &'a self,
        filters: &'a Filters,
    ) -> impl Iterator<Item = (usize, &PluginCommon)> + 'a {
        // Check a few filters before we start do do anything.
        let matches = !filters.wants_user_presets_only()
            && !filters.wants_favorites_only()
            && !filters.advanced_filters_are_set_to_concrete_value();
        if !matches {
            return Either::Left(iter::empty());
        }
        let iter = self
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| filters.product_matches(p.product_id));
        Either::Right(iter)
    }
}

impl Database for DefaultsDatabase {
    fn filter_item_name(&self) -> String {
        "FX defaults".to_string()
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        // We clone the plug-in list so we can create own own order and maintain stable IDs.
        self.plugins = ctx.plugin_db.plugins().map(|p| p.common.clone()).collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &ProviderContext,
        input: &BuildInput,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut filter_settings = input.filter_settings;
        filter_settings.clear_this_and_dependent_filters(PotFilterItemKind::Bank);
        let product_items = self
            .query_presets_internal(&filter_settings)
            .filter_map(|(_, plugin)| Some(plugin.product_id))
            .unique()
            .map(InnerFilterItem::Product)
            .collect();
        let mut collections = InnerFilterItemCollections::empty();
        collections.set(PotFilterItemKind::Bank, product_items);
        Ok(collections)
    }

    fn query_presets(
        &self,
        _: &ProviderContext,
        input: &BuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        if !input.search_evaluator.matches(PRESET_NAME) {
            return Ok(vec![]);
        }
        let preset_ids = self
            .query_presets_internal(&input.filter_settings)
            .map(|(i, _)| SortablePresetId::new(i as _, PRESET_NAME.to_string()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, _: &ProviderContext, preset_id: InnerPresetId) -> Option<Preset> {
        let plugin = self.plugins.get(preset_id.0 as usize)?;
        let preset = Preset {
            common: PresetCommon {
                favorite_id: "".to_string(),
                name: PRESET_NAME.to_string(),
                product_name: Some(plugin.to_string()),
            },
            kind: PresetKind::DefaultFactory(plugin.id),
        };
        Some(preset)
    }

    fn find_preview_by_preset_id(
        &self,
        _: &ProviderContext,
        _preset_id: InnerPresetId,
    ) -> Option<PathBuf> {
        None
    }
}

const PRESET_NAME: &str = "<Default>";
