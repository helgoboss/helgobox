use crate::domain::pot::provider_database::{
    Database, InnerFilterItem, InnerFilterItemCollections, ProviderContext, SortablePresetId,
};
use crate::domain::pot::{
    BuildInput, Filters, InnerPresetId, PotFilterExcludeList, Preset, PresetCommon, PresetKind,
};

use crate::domain::pot::plugins::PluginCommon;
use either::Either;
use enumset::{enum_set, EnumSet};
use itertools::Itertools;
use realearn_api::persistence::PotFilterKind;
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
        excludes: &'a PotFilterExcludeList,
    ) -> impl Iterator<Item = (usize, &PluginCommon)> + 'a {
        let matches = !filters.wants_user_presets_only()
            && !filters.wants_favorites_only()
            && !filters.any_filter_below_is_set_to_concrete_value(PotFilterKind::Bank);
        if !matches {
            return Either::Left(iter::empty());
        }
        let iter = self
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| filters.plugin_core_matches(&p.core, excludes));
        Either::Right(iter)
    }
}

impl Database for DefaultsDatabase {
    fn name(&self) -> String {
        "FX defaults".to_string()
    }

    fn description(&self) -> String {
        "Default factory presets for all of your plug-ins".to_string()
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
        input: &BuildInput,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>> {
        let mut filter_settings = input.filter_settings;
        // TODO-high Respect global exclusions
        filter_settings.clear_this_and_dependent_filters(PotFilterKind::Bank);
        let product_items = self
            .query_presets_internal(&filter_settings, &input.filter_exclude_list)
            .filter_map(|(_, plugin)| Some(plugin.core.product_id))
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
        input: &BuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        if !input.search_evaluator.matches(PRESET_NAME) {
            return Ok(vec![]);
        }
        let preset_ids = self
            .query_presets_internal(&input.filter_settings, &input.filter_exclude_list)
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
            kind: PresetKind::DefaultFactory(plugin.core.id),
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
