use crate::domain::pot::provider_database::DatabaseId;
use crate::domain::pot::{FilterItem, Preset};
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;
use enumset::EnumSet;
use realearn_api::persistence::PotFilterItemKind;
use std::collections::HashSet;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(
    Copy, Clone, Eq, PartialEq, Hash, Debug, Default, serde::Serialize, serde::Deserialize,
)]
pub struct FilterItemId(pub Option<u32>);

impl FilterItemId {
    pub const NONE: Self = Self(None);
}

#[derive(Debug, Default)]
pub struct FilterItemCollections(EnumMap<PotFilterItemKind, Vec<FilterItem>>);

impl FilterItemCollections {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn get(&self, kind: PotFilterItemKind) -> &[FilterItem] {
        &self.0[kind]
    }

    pub fn into_iter(self) -> impl Iterator<Item = (PotFilterItemKind, Vec<FilterItem>)> {
        self.0.into_iter()
    }

    pub fn set(&mut self, kind: PotFilterItemKind, items: Vec<FilterItem>) {
        self.0[kind] = items;
    }

    pub fn extend(&mut self, kind: PotFilterItemKind, items: impl Iterator<Item = FilterItem>) {
        self.0[kind].extend(items);
    }

    pub fn narrow_down(&mut self, kind: PotFilterItemKind, includes: &HashSet<FilterItemId>) {
        self.0[kind].retain(|item| includes.contains(&item.id))
    }
}

/// `Some` means a filter is set (can also be the `<None>` filter).
/// `None` means no filter is set (`<Any>`).
pub type OptFilter = Option<FilterItemId>;

#[derive(Copy, Clone, Debug, Default)]
pub struct Filters(EnumMap<PotFilterItemKind, OptFilter>);

impl Filters {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn database_matches(&self, db_id: DatabaseId) -> bool {
        if let Some(FilterItemId(Some(filter_db_id))) = self.get(PotFilterItemKind::Database) {
            db_id.0 == filter_db_id
        } else {
            true
        }
    }

    /// Returns `false` if set to `None`
    pub fn is_set_to_concrete_value(&self, kind: PotFilterItemKind) -> bool {
        matches!(self.0[kind], Some(FilterItemId(Some(_))))
    }

    pub fn get(&self, kind: PotFilterItemKind) -> OptFilter {
        self.0[kind]
    }

    pub fn get_ref(&self, kind: PotFilterItemKind) -> &OptFilter {
        &self.0[kind]
    }

    pub fn set(&mut self, kind: PotFilterItemKind, value: OptFilter) {
        self.0[kind] = value;
    }

    pub fn effective_sub_bank(&self) -> &OptFilter {
        self.effective_sub_item(PotFilterItemKind::NksBank, PotFilterItemKind::NksSubBank)
    }

    pub fn clear_excluded_ones(&mut self, exclude_list: &PotFilterExcludeList) {
        for kind in PotFilterItemKind::into_enum_iter() {
            if let Some(id) = self.0[kind] {
                if exclude_list.contains(kind, id) {
                    self.0[kind] = None;
                }
            }
        }
    }

    pub fn clear_if_not_available_anymore(
        &mut self,
        affected_kinds: EnumSet<PotFilterItemKind>,
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

    pub fn iter(&self) -> impl Iterator<Item = (PotFilterItemKind, OptFilter)> {
        self.0.into_iter()
    }

    pub fn effective_sub_category(&self) -> &OptFilter {
        self.effective_sub_item(
            PotFilterItemKind::NksCategory,
            PotFilterItemKind::NksSubCategory,
        )
    }

    fn effective_sub_item(
        &self,
        parent_kind: PotFilterItemKind,
        sub_kind: PotFilterItemKind,
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
    exluded_items: EnumMap<PotFilterItemKind, HashSet<FilterItemId>>,
}

impl PotFilterExcludeList {
    pub fn contains(&self, kind: PotFilterItemKind, id: FilterItemId) -> bool {
        self.exluded_items[kind].contains(&id)
    }

    pub fn include(&mut self, kind: PotFilterItemKind, id: FilterItemId) {
        self.exluded_items[kind].remove(&id);
    }

    pub fn exclude(&mut self, kind: PotFilterItemKind, id: FilterItemId) {
        self.exluded_items[kind].insert(id);
    }

    pub fn has_excludes(&self, kind: PotFilterItemKind) -> bool {
        !self.exluded_items[kind].is_empty()
    }

    pub fn normal_excludes_by_kind(
        &self,
        kind: PotFilterItemKind,
    ) -> impl Iterator<Item = &u32> + '_ {
        self.exluded_items[kind]
            .iter()
            .filter_map(|id| Some(id.0.as_ref()?))
    }

    pub fn contains_none(&self, kind: PotFilterItemKind) -> bool {
        self.exluded_items[kind].contains(&FilterItemId::NONE)
    }
}

#[derive(Debug)]
pub struct CurrentPreset {
    preset: Preset,
    macro_param_banks: Vec<MacroParamBank>,
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
    pub fn without_parameters(preset: Preset) -> Self {
        Self {
            preset,
            macro_param_banks: Default::default(),
        }
    }

    pub fn with_parameters(preset: Preset, macro_param_banks: Vec<MacroParamBank>) -> Self {
        Self {
            preset,
            macro_param_banks,
        }
    }

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
