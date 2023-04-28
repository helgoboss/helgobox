use crate::domain::pot::plugins::{PluginCore, ProductKind};
use crate::domain::pot::provider_database::{
    DatabaseId, FIL_IS_FAVORITE_TRUE, FIL_IS_USER_PRESET_FALSE, FIL_IS_USER_PRESET_TRUE,
};
use crate::domain::pot::{FilterItem, Preset};
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;
use enumset::EnumSet;
use realearn_api::persistence::PotFilterKind;
use std::collections::HashSet;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PresetId {
    pub database_id: DatabaseId,
    pub preset_id: InnerPresetId,
}

impl PresetId {
    pub fn new(database_id: DatabaseId, preset_id: InnerPresetId) -> Self {
        Self {
            database_id,
            preset_id,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct InnerPresetId(pub u32);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct FilterItemId(pub Option<Fil>);

impl FilterItemId {
    pub const NONE: Self = Self(None);
}

/// Filter value.
///
/// These can be understood as possible types of a filter item kind. Not all types make sense
/// for a particular filter item kind.
///
/// Many of these types are not suitable for persistence because their values are not stable.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Fil {
    /// A typical integer filter value to refer to a filter item in a specific Komplete database.
    ///
    /// This needs a pot filter item kind and a specific Komplete database to make full sense.
    /// The integers are not suited for being persisted because different Komplete scans can yield
    /// different integers! So they should only be used at runtime and translated to something
    /// more stable for persistence.
    Komplete(u32),
    /// Refers to a specific pot database.
    ///
    /// Only valid at runtime, not suitable for persistence.
    Database(DatabaseId),
    /// Refers to something that can be true of false, e.g. "favorite" or "not favorite"
    /// or "available" or "not available".
    ///
    /// Suitable for persistence.
    Boolean(bool),
    /// Refers to a kind of product.
    ProductKind(ProductKind),
    /// Refers to a product.
    ///
    /// Not suitable for persistence because the product IDs are created at runtime.
    Product(ProductId),
}

/// Id for a [`Product`].
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ProductId(pub u32);

pub type FilterItemCollections = GenericFilterItemCollections<FilterItem>;

#[derive(Debug)]
pub struct GenericFilterItemCollections<T>(EnumMap<PotFilterKind, Vec<T>>);

pub trait HasFilterItemId {
    fn id(&self) -> FilterItemId;
}

impl HasFilterItemId for FilterItem {
    fn id(&self) -> FilterItemId {
        self.id
    }
}

impl<T> Default for GenericFilterItemCollections<T> {
    fn default() -> Self {
        Self(enum_map::enum_map! { _ => vec![] })
    }
}

impl<T> GenericFilterItemCollections<T> {
    pub fn empty() -> Self {
        Default::default()
    }

    pub fn get(&self, kind: PotFilterKind) -> &[T] {
        &self.0[kind]
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (PotFilterKind, &mut Vec<T>)> {
        self.0.iter_mut()
    }

    pub fn into_iter(self) -> impl Iterator<Item = (PotFilterKind, Vec<T>)> {
        self.0.into_iter()
    }

    pub fn set(&mut self, kind: PotFilterKind, items: Vec<T>) {
        self.0[kind] = items;
    }

    pub fn extend(&mut self, kind: PotFilterKind, items: impl Iterator<Item = T>) {
        self.0[kind].extend(items);
    }
}

impl<T: HasFilterItemId> GenericFilterItemCollections<T> {
    pub fn narrow_down(&mut self, kind: PotFilterKind, includes: &HashSet<FilterItemId>) {
        self.0[kind].retain(|item| includes.contains(&item.id()))
    }
}

/// `Some` means a filter is set (can also be the `<None>` filter).
/// `None` means no filter is set (`<Any>`).
pub type OptFilter = Option<FilterItemId>;

#[derive(Copy, Clone, Debug, Default)]
pub struct Filters(EnumMap<PotFilterKind, OptFilter>);

impl Filters {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn database_matches(&self, db_id: DatabaseId) -> bool {
        self.matches(PotFilterKind::Database, Fil::Database(db_id))
    }

    pub fn plugin_core_matches(&self, core: &PluginCore) -> bool {
        self.matches_optional(
            PotFilterKind::ProductKind,
            core.product_kind.map(Fil::ProductKind),
        ) && self.matches_optional(PotFilterKind::Bank, Some(Fil::Product(core.product_id)))
    }

    pub fn wants_user_presets_only(&self) -> bool {
        self.wants_only(PotFilterKind::IsUser, FIL_IS_USER_PRESET_TRUE)
    }

    pub fn wants_factory_presets_only(&self) -> bool {
        self.wants_only(PotFilterKind::IsUser, FIL_IS_USER_PRESET_FALSE)
    }

    pub fn wants_favorites_only(&self) -> bool {
        self.wants_only(PotFilterKind::IsFavorite, FIL_IS_FAVORITE_TRUE)
    }

    pub fn any_filter_below_is_set_to_concrete_value(&self, kind: PotFilterKind) -> bool {
        kind.dependent_kinds()
            .any(|k| self.is_set_to_concrete_value(k))
    }

    /// To be used with filter kinds where <None> is **not** a valid filter value. In this case,
    /// <None> is considered an invalid value and it never matches (no reason to panic but almost).
    fn matches(&self, kind: PotFilterKind, fil: Fil) -> bool {
        match self.get(kind) {
            None => true,
            Some(FilterItemId(None)) => false,
            Some(FilterItemId(Some(wanted_fil))) => fil == wanted_fil,
        }
    }

    /// To be used with filter kinds where <None> is a valid filter value.
    fn matches_optional(&self, kind: PotFilterKind, fil: Option<Fil>) -> bool {
        match self.get(kind) {
            None => true,
            Some(FilterItemId(wanted_fil)) => fil == wanted_fil,
        }
    }

    fn wants_only(&self, kind: PotFilterKind, fil: Fil) -> bool {
        self.get(kind) == Some(FilterItemId(Some(fil)))
    }

    /// Returns `false` if set to `None`
    pub fn is_set_to_concrete_value(&self, kind: PotFilterKind) -> bool {
        matches!(self.0[kind], Some(FilterItemId(Some(_))))
    }

    pub fn get(&self, kind: PotFilterKind) -> OptFilter {
        self.0[kind]
    }

    pub fn get_ref(&self, kind: PotFilterKind) -> &OptFilter {
        &self.0[kind]
    }

    pub fn set(&mut self, kind: PotFilterKind, value: OptFilter) {
        self.0[kind] = value;
    }

    pub fn effective_sub_bank(&self) -> &OptFilter {
        self.effective_sub_item(PotFilterKind::Bank, PotFilterKind::SubBank)
    }

    pub fn clear_excluded_ones(&mut self, exclude_list: &PotFilterExcludeList) {
        for kind in PotFilterKind::into_enum_iter() {
            if let Some(id) = self.0[kind] {
                if exclude_list.contains(kind, id) {
                    self.0[kind] = None;
                }
            }
        }
    }

    pub fn clear_if_not_available_anymore(
        &mut self,
        affected_kinds: EnumSet<PotFilterKind>,
        collections: &FilterItemCollections,
    ) {
        for kind in affected_kinds {
            if let Some(id) = self.0[kind] {
                let valid_items = collections.get(kind);
                if !valid_items.iter().any(|item| item.id == id) {
                    self.0[kind] = None;
                }
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (PotFilterKind, OptFilter)> {
        self.0.into_iter()
    }

    pub fn effective_sub_category(&self) -> &OptFilter {
        self.effective_sub_item(PotFilterKind::Category, PotFilterKind::SubCategory)
    }

    pub fn clear_this_and_dependent_filters(&mut self, kind: PotFilterKind) {
        self.set(kind, None);
        for dependent_kind in kind.dependent_kinds() {
            self.set(dependent_kind, None);
        }
    }

    fn effective_sub_item(
        &self,
        parent_kind: PotFilterKind,
        sub_kind: PotFilterKind,
    ) -> &OptFilter {
        let category = &self.0[parent_kind];
        if category == &Some(FilterItemId::NONE) {
            &category
        } else {
            &self.0[sub_kind]
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PotFilterExcludeList {
    exluded_items: EnumMap<PotFilterKind, HashSet<FilterItemId>>,
}

impl PotFilterExcludeList {
    pub fn contains(&self, kind: PotFilterKind, id: FilterItemId) -> bool {
        self.exluded_items[kind].contains(&id)
    }

    pub fn include(&mut self, kind: PotFilterKind, id: FilterItemId) {
        self.exluded_items[kind].remove(&id);
    }

    pub fn exclude(&mut self, kind: PotFilterKind, id: FilterItemId) {
        self.exluded_items[kind].insert(id);
    }

    pub fn has_excludes(&self, kind: PotFilterKind) -> bool {
        !self.exluded_items[kind].is_empty()
    }

    pub fn excludes_database(&self, db_id: DatabaseId) -> bool {
        self.contains(
            PotFilterKind::Database,
            FilterItemId(Some(Fil::Database(db_id))),
        )
    }

    pub fn normal_excludes_by_kind(&self, kind: PotFilterKind) -> impl Iterator<Item = &Fil> + '_ {
        self.exluded_items[kind]
            .iter()
            .filter_map(|id| Some(id.0.as_ref()?))
    }

    pub fn contains_none(&self, kind: PotFilterKind) -> bool {
        self.exluded_items[kind].contains(&FilterItemId::NONE)
    }
}

#[derive(Debug)]
pub struct CurrentPreset {
    pub preset: Preset,
    pub macro_param_banks: Vec<MacroParamBank>,
}

#[derive(Debug)]
pub struct MacroParamBank {
    params: Vec<MacroParam>,
}

impl MacroParamBank {
    pub fn new(params: Vec<MacroParam>) -> Self {
        Self { params }
    }

    pub fn name(&self) -> String {
        let mut name = String::with_capacity(32);
        for p in &self.params {
            if !p.section_name.is_empty() {
                if !name.is_empty() {
                    name += " / ";
                }
                name += &p.section_name;
            }
        }
        name
    }

    pub fn params(&self) -> &[MacroParam] {
        &self.params
    }

    pub fn find_macro_param_at(&self, slot_index: u32) -> Option<&MacroParam> {
        self.params.get(slot_index as usize)
    }

    pub fn param_count(&self) -> u32 {
        self.params.len() as _
    }
}

#[derive(Clone, Debug)]
pub struct MacroParam {
    pub name: String,
    pub section_name: String,
    pub param_index: Option<u32>,
}

impl CurrentPreset {
    pub fn preset(&self) -> &Preset {
        &self.preset
    }

    pub fn find_macro_param_bank_at(&self, bank_index: u32) -> Option<&MacroParamBank> {
        self.macro_param_banks.get(bank_index as usize)
    }

    pub fn find_macro_param_at(&self, slot_index: u32) -> Option<&MacroParam> {
        let bank_index = slot_index / 8;
        let bank_slot_index = slot_index % 8;
        self.find_bank_macro_param_at(bank_index, bank_slot_index)
    }

    pub fn find_bank_macro_param_at(
        &self,
        bank_index: u32,
        bank_slot_index: u32,
    ) -> Option<&MacroParam> {
        self.macro_param_banks
            .get(bank_index as usize)?
            .find_macro_param_at(bank_slot_index)
    }

    pub fn macro_param_bank_count(&self) -> u32 {
        self.macro_param_banks.len() as _
    }

    pub fn has_params(&self) -> bool {
        self.macro_param_banks.len() > 0
    }
}
