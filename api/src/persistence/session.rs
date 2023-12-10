use crate::persistence::*;

/// Only used for JSON schema generation at the moment.
pub struct Session {
    _main_compartment: Option<Compartment>,
    #[cfg(feature = "playtime")]
    _clip_matrix: Option<playtime_api::persistence::Matrix>,
    _mapping_snapshots: Vec<MappingSnapshot>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MappingSnapshot {
    pub id: String,
    pub mappings: Vec<MappingInSnapshot>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MappingInSnapshot {
    pub id: String,
    pub target_value: TargetValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TargetValue {
    #[serde(alias = "Normalized")]
    Unit {
        value: f64,
    },
    Discrete {
        value: u32,
    },
}
