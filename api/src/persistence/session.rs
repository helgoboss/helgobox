use crate::persistence::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// TODO-high This is incorrect and at the moment only used for JSON Schema generation.
#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Session {
    #[serde(skip_serializing_if = "Option::is_none")]
    main_compartment: Option<Compartment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    clip_matrix: Option<playtime_api::Matrix>,
}
