use crate::domain::SmallAsciiString;
use ascii::AsciiString;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ControlElementId {
    Indexed(u32),
    Named(String),
}

impl Default for ControlElementId {
    fn default() -> Self {
        ControlElementId::Indexed(0)
    }
}

impl ControlElementId {
    pub fn from_index_and_name(index: Option<u32>, name: &AsciiString) -> Self {
        if let Some(i) = index {
            ControlElementId::Indexed(i)
        } else {
            ControlElementId::Named(name.to_string())
        }
    }

    pub fn index(&self) -> Option<u32> {
        match self {
            ControlElementId::Indexed(i) => Some(*i),
            ControlElementId::Named(_) => None,
        }
    }

    pub fn name(&self) -> AsciiString {
        match self {
            ControlElementId::Indexed(_) => AsciiString::new(),
            ControlElementId::Named(n) => SmallAsciiString::create_compatible_ascii_string(n),
        }
    }
}
