use crate::domain::VirtualControlElementId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VirtualControlElementIdData {
    Indexed(u32),
    Named(String),
}

impl Default for VirtualControlElementIdData {
    fn default() -> Self {
        VirtualControlElementIdData::Indexed(0)
    }
}

impl VirtualControlElementIdData {
    pub fn from_model(model: VirtualControlElementId) -> Self {
        match model {
            VirtualControlElementId::Indexed(i) => VirtualControlElementIdData::Indexed(i),
            VirtualControlElementId::Named(n) => VirtualControlElementIdData::Named(n.to_string()),
        }
    }

    pub fn to_model(&self) -> VirtualControlElementId {
        match self {
            VirtualControlElementIdData::Indexed(i) => VirtualControlElementId::Indexed(*i),
            VirtualControlElementIdData::Named(n) => n.parse().unwrap_or_default(),
        }
    }
}
