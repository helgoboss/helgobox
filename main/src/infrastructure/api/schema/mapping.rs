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
    pub activation_condition: Option<ActivationCondition>,
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
    Todo,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind")]
pub enum ActivationCondition {
    Modifier(ModifierActivationCondition),
    Bank(BankActivationCondition),
    Eel(EelActivationCondition),
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct ModifierActivationCondition {
    pub modifiers: Vec<ModifierState>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct ModifierState {
    pub parameter: ParamRef,
    pub on: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct BankActivationCondition {
    pub parameter: ParamRef,
    pub bank_index: u32,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct EelActivationCondition {
    pub condition: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum ParamRef {
    Index(u32),
    Key(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum VirtualControlElementId {
    Indexed(u32),
    Named(String),
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum VirtualControlElementKind {
    Multi,
    Button,
}

impl Default for VirtualControlElementKind {
    fn default() -> Self {
        Self::Multi
    }
}

#[derive(Copy, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub struct OscArgument {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<OscArgKind>,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum OscArgKind {
    Float,
    Double,
    Bool,
    Nil,
    Inf,
    Int,
    String,
    Blob,
    Time,
    Long,
    Char,
    Color,
    Midi,
    Array,
}

impl Default for OscArgKind {
    fn default() -> Self {
        Self::Float
    }
}
