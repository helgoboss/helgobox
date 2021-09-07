use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

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
pub enum MappingScope {
    #[serde(rename = "instance")]
    #[display(fmt = "All mappings in instance")]
    AllInInstance,
    #[serde(rename = "instance-active")]
    #[display(fmt = "All active mappings in instance")]
    AllActiveInInstance,
    #[serde(rename = "group")]
    #[display(fmt = "All mappings in group")]
    AllInGroup,
    #[serde(rename = "group-active")]
    #[display(fmt = "All active mappings in group")]
    AllActiveInGroup,
}

impl MappingScope {
    pub fn active_mappings_only(self) -> bool {
        use MappingScope::*;
        matches!(self, AllActiveInInstance | AllActiveInGroup)
    }
}

impl Default for MappingScope {
    fn default() -> Self {
        Self::AllInInstance
    }
}
