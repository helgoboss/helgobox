use crate::infrastructure::api::schema::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Group {
    /// An optional key that you can assign to this group in order to refer
    /// to it from somewhere else.
    ///
    /// This key should be unique within this list of groups.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
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
