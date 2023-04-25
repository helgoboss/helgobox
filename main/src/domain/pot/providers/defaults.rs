use crate::domain::pot::provider_database::{
    build_product_name_from_plugin_info, Database, ProviderContext, SortablePresetId,
    FIL_CONTENT_TYPE_USER, FIL_FAVORITE_FAVORITE,
};
use crate::domain::pot::{
    BuildInput, FilterItemCollections, FilterItemId, InnerPresetId, Preset, PresetCommon,
    PresetKind,
};

use realearn_api::persistence::PotFilterItemKind;
use std::error::Error;
use std::path::PathBuf;
use wildmatch::WildMatch;

#[derive(Default)]
pub struct DefaultsDatabase {
    presets: Vec<Preset>,
}

impl DefaultsDatabase {
    pub fn open() -> Self {
        Default::default()
    }
}

impl Database for DefaultsDatabase {
    fn filter_item_name(&self) -> String {
        "Default FX presets".to_string()
    }

    fn refresh(&mut self, ctx: &ProviderContext) -> Result<(), Box<dyn Error>> {
        self.presets = ctx
            .plugins
            .iter()
            .filter_map(|p| {
                let plugin_id = p.kind.plugin_id().ok()?;
                let preset = Preset {
                    common: PresetCommon {
                        favorite_id: "".to_string(),
                        name: "<Default>".to_string(),
                        product_name: Some(build_product_name_from_plugin_info(&p.name, plugin_id)),
                    },
                    kind: PresetKind::Default(plugin_id),
                };
                Some(preset)
            })
            .collect();
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
        let lowercase_search_expression = input.search_expression.trim().to_lowercase();
        let wild_match = WildMatch::new(&lowercase_search_expression);
        let preset_ids = self
            .presets
            .iter()
            .enumerate()
            .filter_map(|(i, preset)| {
                let id = InnerPresetId(i as u32);
                let matches = if lowercase_search_expression.is_empty() {
                    true
                } else {
                    let lowercase_preset_name = preset.common.name.to_lowercase();
                    if input.use_wildcard_search {
                        wild_match.matches(&lowercase_preset_name)
                    } else {
                        lowercase_preset_name.contains(&lowercase_search_expression)
                    }
                };
                if matches {
                    Some(SortablePresetId::new(id, preset.common.name.clone()))
                } else {
                    None
                }
            })
            .collect();
        Ok(preset_ids)
    }

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset> {
        self.presets.get(preset_id.0 as usize).cloned()
    }

    fn find_preview_by_preset_id(&self, _preset_id: InnerPresetId) -> Option<PathBuf> {
        None
    }
}
