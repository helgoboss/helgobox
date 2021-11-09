use crate::schema::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// An optional ID that you can assign to this group in order to refer
    /// to it from somewhere else.
    ///
    /// This ID should be unique within all groups in the same compartment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_condition: Option<ActivationCondition>,
}
