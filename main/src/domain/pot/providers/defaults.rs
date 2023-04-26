use crate::domain::pot::provider_database::{
    Database, ProviderContext, SortablePresetId, FIL_CONTENT_TYPE_USER, FIL_FAVORITE_FAVORITE,
};
use crate::domain::pot::{
    BuildInput, FilterItemCollections, FilterItemId, InnerPresetId, Preset, PresetCommon,
    PresetKind,
};

use crate::domain::pot::plugins::PluginCommon;
use realearn_api::persistence::PotFilterItemKind;
use std::error::Error;
use std::path::PathBuf;

#[derive(Default)]
pub struct DefaultsDatabase {
    plugins: Vec<PluginCommon>,
}

impl DefaultsDatabase {
    pub fn open() -> Self {
        Default::default()
    }
}

impl Database for DefaultsDatabase {
    fn filter_item_name(&self) -> String {
        "FX defaults".to_string()
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        self.plugins = ctx.plugin_db.plugins().map(|p| p.common.clone()).collect();
        Ok(())
    }

    fn query_filter_collections(
        &self,
        _: &BuildInput,
    ) -> Result<FilterItemCollections, Box<dyn Error>> {
        Ok(FilterItemCollections::empty())
    }

    fn query_presets(&self, input: &BuildInput) -> Result<Vec<SortablePresetId>, Box<dyn Error>> {
        for (kind, filter) in input.filter_settings.iter() {
            use PotFilterItemKind::*;
            let matches = match kind {
                IsUser => filter != Some(FilterItemId(Some(FIL_CONTENT_TYPE_USER))),
                IsFavorite => filter != Some(FilterItemId(Some(FIL_FAVORITE_FAVORITE))),
                ProductKind | Bank | SubBank | Category | SubCategory | Mode => {
                    matches!(filter, None | Some(FilterItemId::NONE))
                }
                _ => true,
            };
            if !matches {
                return Ok(vec![]);
            }
        }
        if !input.search_evaluator.matches(PRESET_NAME) {
            return Ok(vec![]);
        }
        let preset_ids = (0..self.plugins.len())
            .map(|i| SortablePresetId::new(i as _, PRESET_NAME.to_string()))
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset> {
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

    fn find_preview_by_preset_id(&self, _preset_id: InnerPresetId) -> Option<PathBuf> {
        None
    }
}

const PRESET_NAME: &str = "<Default>";
