use crate::domain::pot::plugins::{PluginDatabase, ProductKind};
use crate::domain::pot::{
    BuildInput, Fil, FilterItem, FilterItemId, GenericFilterItemCollections, HasFilterItemId,
    InnerPresetId, Preset, ProductId,
};
use std::error::Error;
use std::path::PathBuf;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DatabaseId(pub u32);

pub trait Database {
    fn filter_item_name(&self) -> String;

    fn refresh(&mut self, context: &ProviderContext) -> Result<(), Box<dyn Error>>;

    fn query_filter_collections(
        &self,
        context: &ProviderContext,
        input: &BuildInput,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>>;

    fn query_presets(
        &self,
        context: &ProviderContext,
        input: &BuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>>;

    fn find_preset_by_id(
        &self,
        context: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<Preset>;

    fn find_preview_by_preset_id(
        &self,
        context: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<PathBuf>;
}

pub type InnerFilterItemCollections = GenericFilterItemCollections<InnerFilterItem>;

pub enum InnerFilterItem {
    /// A unique final filter item. Only makes sense within a specific database and within the
    /// context of a specific pot filter item kind. Not deduplicated.
    Unique(FilterItem),
    /// A filter item representing a particular product (product for which the preset is made).
    /// Will be deduplicated by the pot database!
    Product(ProductId),
}

impl HasFilterItemId for InnerFilterItem {
    fn id(&self) -> FilterItemId {
        match self {
            InnerFilterItem::Unique(i) => i.id,
            InnerFilterItem::Product(i) => FilterItemId(Some(Fil::Product(*i))),
        }
    }
}

pub struct SortablePresetId {
    pub inner_preset_id: InnerPresetId,
    pub preset_name: String,
}

impl SortablePresetId {
    pub fn new(i: u32, preset_name: String) -> Self {
        Self {
            inner_preset_id: InnerPresetId(i),
            preset_name,
        }
    }
}

pub struct ProviderContext<'a> {
    pub plugin_db: &'a PluginDatabase,
}

impl<'a> ProviderContext<'a> {
    pub fn new(plugin_db: &'a PluginDatabase) -> Self {
        Self { plugin_db }
    }
}

/// Komplete ID = 1
pub const FIL_IS_USER_PRESET_TRUE: Fil = Fil::Boolean(true);
/// Komplete ID = 2
pub const FIL_IS_USER_PRESET_FALSE: Fil = Fil::Boolean(false);
/// Komplete ID = 1
pub const FIL_IS_FAVORITE_TRUE: Fil = Fil::Boolean(true);
/// Komplete ID = 2
pub const FIL_IS_FAVORITE_FALSE: Fil = Fil::Boolean(false);
/// Komplete ID = 1
pub const FIL_PRODUCT_KIND_INSTRUMENT: Fil = Fil::ProductKind(ProductKind::Instrument);
/// Komplete ID = 2
pub const FIL_PRODUCT_KIND_EFFECT: Fil = Fil::ProductKind(ProductKind::Effect);
/// Komplete ID = 4
pub const FIL_PRODUCT_KIND_LOOP: Fil = Fil::ProductKind(ProductKind::Loop);
/// Komplete ID = 8
pub const FIL_PRODUCT_KIND_ONE_SHOT: Fil = Fil::ProductKind(ProductKind::OneShot);
