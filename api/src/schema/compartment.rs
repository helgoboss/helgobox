use crate::schema::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Compartment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_group: Option<Group>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Group>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Vec<Mapping>>,
    /// At the moment, custom data is only used in the controller compartment.
    ///
    /// Attention: Custom data in the main compartment is not fully supported, i.e.
    /// it's not saved together with the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<HashMap<String, serde_json::Value>>,
}
