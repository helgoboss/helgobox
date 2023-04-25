use crate::domain::pot::plugins::{Plugin, ProductKind};
use crate::domain::pot::{BuildInput, Fil, FilterItemCollections, InnerPresetId, PluginId, Preset};
use std::error::Error;
use std::path::PathBuf;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DatabaseId(pub u32);

pub trait Database {
    fn filter_item_name(&self) -> String;

    fn refresh(&mut self, context: &ProviderContext) -> Result<(), Box<dyn Error>>;

    fn query_filter_collections(
        &self,
        input: &BuildInput,
    ) -> Result<FilterItemCollections, Box<dyn Error>>;

    fn query_presets(&self, input: &BuildInput) -> Result<Vec<SortablePresetId>, Box<dyn Error>>;

    fn find_preset_by_id(&self, preset_id: InnerPresetId) -> Option<Preset>;

    fn find_preview_by_preset_id(&self, preset_id: InnerPresetId) -> Option<PathBuf>;
}

pub struct SortablePresetId {
    pub inner_preset_id: InnerPresetId,
    pub preset_name: String,
}

impl SortablePresetId {
    pub fn new(inner_preset_id: InnerPresetId, preset_name: String) -> Self {
        Self {
            inner_preset_id,
            preset_name,
        }
    }
}

pub struct ProviderContext<'a> {
    pub plugins: &'a [Plugin],
}

/// Komplete ID = 1
pub const FIL_CONTENT_TYPE_USER: Fil = Fil::Boolean(true);
/// Komplete ID = 2
pub const FIL_CONTENT_TYPE_FACTORY: Fil = Fil::Boolean(false);
/// Komplete ID = 1
pub const FIL_FAVORITE_FAVORITE: Fil = Fil::Boolean(true);
/// Komplete ID = 2
pub const FIL_FAVORITE_NOT_FAVORITE: Fil = Fil::Boolean(false);
/// Komplete ID = 1
pub const FIL_PRODUCT_KIND_INSTRUMENT: Fil = Fil::ProductKind(ProductKind::Instrument);
/// Komplete ID = 2
pub const FIL_PRODUCT_KIND_EFFECT: Fil = Fil::ProductKind(ProductKind::Effect);
/// Komplete ID = 4
pub const FIL_PRODUCT_KIND_LOOP: Fil = Fil::ProductKind(ProductKind::Loop);
/// Komplete ID = 8
pub const FIL_PRODUCT_KIND_ONE_SHOT: Fil = Fil::ProductKind(ProductKind::OneShot);

// TODO-high CONTINUE Also integrate info if instrument or effect, best use struct PluginInfo in Preset directly
pub fn build_product_name_from_plugin_info(plugin_name: &str, plugin_id: PluginId) -> String {
    format!("{}: {}", plugin_id.kind_name(), plugin_name)
}
