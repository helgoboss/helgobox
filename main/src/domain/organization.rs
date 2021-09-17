use crate::domain::{GroupId, MainMapping, Tag};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct MappingScope {
    pub universe: MappingUniverse,
    /// The mapping in question should have at least one of these tags.
    pub tags: HashSet<Tag>,
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

    pub fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }

    /// A mapping matches the tags if it has at least one of the tags of this scope.
    ///
    /// If the scope has no tags at all, then any mapping matches.
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
}

impl MappingUniverse {
    pub fn matches(&self, m: &MainMapping, required_group_id: GroupId) -> bool {
        if *self == MappingUniverse::AllInGroup && m.group_id() != required_group_id {
            return false;
        }
        true
    }
}

impl Default for MappingUniverse {
    fn default() -> Self {
        Self::AllInCompartment
    }
}
