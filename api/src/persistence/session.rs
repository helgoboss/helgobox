use crate::persistence::*;

/// Only used for JSON schema generation at the moment.
pub struct Session {
    _main_compartment: Option<Compartment>,
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
