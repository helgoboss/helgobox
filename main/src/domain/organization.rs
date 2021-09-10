use crate::domain::{GroupId, MainMapping, Tag};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct FullMappingScope {
    pub scope: MappingScope,
    /// The mapping in question should have at least one of these tags.
    pub tags: Vec<Tag>,
}

impl FullMappingScope {
    pub fn matches(&self, m: &MainMapping, required_group_id: GroupId) -> bool {
        if self.scope.active_mappings_only() && !m.is_active() {
            return false;
        }
        if self.scope.mappings_in_group_only() && m.group_id() != required_group_id {
            return false;
        }
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
pub enum MappingScope {
    #[serde(rename = "instance")]
    #[display(fmt = "All mappings in instance")]
    AllInInstance,
    #[serde(rename = "group")]
    #[display(fmt = "All mappings in group")]
    AllInGroup,
    #[serde(rename = "instance-active")]
    #[display(fmt = "All active mappings in instance")]
    AllActiveInInstance,
    #[serde(rename = "group-active")]
    #[display(fmt = "All active mappings in group")]
    AllActiveInGroup,
}

impl MappingScope {
    fn active_mappings_only(self) -> bool {
        use MappingScope::*;
        matches!(self, AllActiveInInstance | AllActiveInGroup)
    }

    fn mappings_in_group_only(self) -> bool {
        use MappingScope::*;
        matches!(self, AllInGroup | AllActiveInGroup)
    }
}

impl Default for MappingScope {
    fn default() -> Self {
        Self::AllInInstance
    }
}
