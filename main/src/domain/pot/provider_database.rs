use crate::domain::pot::{BuildInput, FilterItemCollections, InnerPresetId, Preset};
use std::error::Error;
use std::path::PathBuf;

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, serde::Serialize, serde::Deserialize,
)]
pub struct DatabaseId(pub u32);

pub trait Database {
    fn filter_item_name(&self) -> String;

    fn refresh(&mut self) -> Result<(), Box<dyn Error>>;

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

pub const CONTENT_TYPE_USER_ID: u32 = 1;
pub const CONTENT_TYPE_FACTORY_ID: u32 = 2;
pub const FAVORITE_FAVORITE_ID: u32 = 1;
pub const FAVORITE_NOT_FAVORITE_ID: u32 = 2;
pub const PRODUCT_TYPE_INSTRUMENT_ID: u32 = 1;
pub const PRODUCT_TYPE_EFFECT_ID: u32 = 2;
pub const PRODUCT_TYPE_LOOP_ID: u32 = 4;
pub const PRODUCT_TYPE_ONE_SHOT_ID: u32 = 8;
