use crate::persistence::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete content of a ReaLearn compartment, including mappings, groups, parameters etc.
#[derive(Default, Serialize, Deserialize)]
pub struct Compartment {
    /// Default group settings.
    ///
    /// Group fields `id` and `name` will be ignored for the default group.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
