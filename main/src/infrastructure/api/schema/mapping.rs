use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// An optional key that you can assign to this mapping in order to refer
    /// to it from somewhere else.
    ///
    /// This key should be unique within this list of mappings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible_in_projection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<Active>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback_behavior: Option<FeedbackBehavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_activate: Option<Lifecycle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_deactivate: Option<Lifecycle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glue: Option<Glue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
pub enum Lifecycle {
    Normal,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub enum FeedbackBehavior {
    Normal,
    SendFeedbackAfterControl,
    PreventEchoFeedback,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
pub enum Active {
    Always,
}
