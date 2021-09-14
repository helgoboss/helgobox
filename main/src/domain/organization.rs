use crate::domain::{GroupId, MainMapping, Tag};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct MappingScope {
    pub universe: MappingUniverse,
    /// The mapping in question should have at least one of these tags.
    pub tags: Vec<Tag>,
}

impl MappingScope {
    pub fn matches(&self, m: &MainMapping, required_group_id: GroupId) -> bool {
        if !self.universe.matches(m, required_group_id) {
            return false;
        }
        if !self.matches_tags(m) {
            return false;
        }
        true
    }

    pub fn matches_tags(&self, m: &MainMapping) -> bool {
        if !self.tags.is_empty() && !m.has_any_tag(&self.tags) {
            return false;
        }
        true
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
#[allow(clippy::enum_variant_names)]
pub enum MappingUniverse {
    #[serde(rename = "compartment")]
    #[display(fmt = "All mappings in compartment")]
    AllInCompartment,
    #[serde(rename = "group")]
    #[display(fmt = "All mappings in group")]
    AllInGroup,
    #[serde(rename = "compartment-active")]
    #[display(fmt = "All active mappings in compartment")]
    AllActiveInCompartment,
    #[serde(rename = "group-active")]
    #[display(fmt = "All active mappings in group")]
    AllActiveInGroup,
}

impl MappingUniverse {
    pub fn matches(&self, m: &MainMapping, required_group_id: GroupId) -> bool {
        if self.active_mappings_only() && !m.is_active() {
            return false;
        }
        if self.mappings_in_group_only() && m.group_id() != required_group_id {
            return false;
        }
        true
    }

    fn active_mappings_only(self) -> bool {
        use MappingUniverse::*;
        matches!(self, AllActiveInCompartment | AllActiveInGroup)
    }

    fn mappings_in_group_only(self) -> bool {
        use MappingUniverse::*;
        matches!(self, AllInGroup | AllActiveInGroup)
    }
}

impl Default for MappingUniverse {
    fn default() -> Self {
        Self::AllInCompartment
    }
}
