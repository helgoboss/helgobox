use crate::plugins::{PluginDatabase, ProductKind};
use crate::{
    Fil, FilterItem, FilterItemId, GenericFilterItemCollections, HasFilterItemId, InnerBuildInput,
    InnerPresetId, PersistentDatabaseId, Preset, ProductId,
};
use enumset::{enum_set, EnumSet};
use realearn_api::persistence::PotFilterKind;
use std::borrow::Cow;
use std::error::Error;

/// A database ID that's only stable during the runtime of ReaLearn.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DatabaseId(pub u32);

pub trait Database {
    fn persistent_id(&self) -> &PersistentDatabaseId;

    // TODO-medium-performace Maybe we should require this to be a reference.
    fn name(&self) -> Cow<str>;

    // TODO-medium-performace Maybe we should require this to be a reference.
    fn description(&self) -> Cow<str>;

    fn supported_advanced_filter_kinds(&self) -> EnumSet<PotFilterKind> {
        enum_set!()
    }

    fn refresh(&mut self, context: &ProviderContext) -> Result<(), Box<dyn Error>>;

    fn query_filter_collections(
        &self,
        context: &ProviderContext,
        input: InnerBuildInput,
        affected_kinds: EnumSet<PotFilterKind>,
    ) -> Result<InnerFilterItemCollections, Box<dyn Error>>;

    fn query_presets(
        &self,
        context: &ProviderContext,
        input: InnerBuildInput,
    ) -> Result<Vec<SortablePresetId>, Box<dyn Error>>;

    fn find_preset_by_id(
        &self,
        context: &ProviderContext,
        preset_id: InnerPresetId,
    ) -> Option<Preset>;

    /// Tries to find a preset that belongs to the given product and has the given name *and*
    /// most importantly a preset file format that can't be loaded by Pot Browser.
    ///
    /// This is used by the preset crawler to identify whether a crawled preset can be used to
    /// make a preset with an unsupported format actually loadable. Only makes sense for Komplete
    /// at the moment because this is the only database which exposes unsupported presets.
    fn find_unsupported_preset_matching(
        &self,
        product_id: ProductId,
        preset_name: &str,
    ) -> Option<Preset> {
        let _ = (product_id, preset_name);
        None
    }
}

pub type InnerFilterItemCollections = GenericFilterItemCollections<InnerFilterItem>;

pub enum InnerFilterItem {
    /// A unique final filter item. Only makes sense within a specific database and within the
    /// context of a specific pot filter item kind. Not deduplicated.
    Unique(FilterItem),
    /// A filter item representing a particular product (product for which the preset is made).
    ///
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

#[derive(Copy, Clone)]
pub struct ProviderContext<'a> {
    pub plugin_db: &'a PluginDatabase,
}

impl<'a> ProviderContext<'a> {
    pub fn new(plugin_db: &'a PluginDatabase) -> Self {
        Self { plugin_db }
    }
}

/// Komplete content path state ID = 1
pub const FIL_IS_AVAILABLE_TRUE: Fil = Fil::Boolean(true);
/// Komplete content path state ID = 4
pub const FIL_IS_AVAILABLE_FALSE: Fil = Fil::Boolean(false);
pub const FIL_IS_SUPPORTED_TRUE: Fil = Fil::Boolean(true);
pub const FIL_IS_SUPPORTED_FALSE: Fil = Fil::Boolean(false);
pub const FIL_IS_FAVORITE_TRUE: Fil = Fil::Boolean(true);
pub const FIL_IS_FAVORITE_FALSE: Fil = Fil::Boolean(false);
/// Komplete content type ID = 1
pub const FIL_IS_USER_PRESET_TRUE: Fil = Fil::Boolean(true);
/// Komplete content type ID = 2
pub const FIL_IS_USER_PRESET_FALSE: Fil = Fil::Boolean(false);
/// Komplete product type ID = 1
pub const FIL_PRODUCT_KIND_INSTRUMENT: Fil = Fil::ProductKind(ProductKind::Instrument);
/// Komplete product type ID = 2
pub const FIL_PRODUCT_KIND_EFFECT: Fil = Fil::ProductKind(ProductKind::Effect);
/// Komplete product type ID = 4
pub const FIL_PRODUCT_KIND_LOOP: Fil = Fil::ProductKind(ProductKind::Loop);
/// Komplete product type ID = 8
pub const FIL_PRODUCT_KIND_ONE_SHOT: Fil = Fil::ProductKind(ProductKind::OneShot);
pub const FIL_HAS_PREVIEW_TRUE: Fil = Fil::Boolean(true);
pub const FIL_HAS_PREVIEW_FALSE: Fil = Fil::Boolean(false);
