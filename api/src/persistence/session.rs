use crate::persistence::*;
use schemars::JsonSchema;

/// Only used for JSON schema generation at the moment.
#[derive(JsonSchema)]
pub struct Session {
    _main_compartment: Option<Compartment>,
    _clip_matrix: Option<Matrix>,
    _mapping_snapshots: Vec<MappingSnapshot>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MappingSnapshot {
    pub id: String,
    pub mappings: Vec<MappingInSnapshot>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MappingInSnapshot {
    pub id: String,
    pub target_value: TargetValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum TargetValue {
    Normalized { value: f64 },
    Discrete { value: u32 },
}
