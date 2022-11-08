use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Mapping {
    /// An optional ID that you can assign to this mapping in order to refer
    /// to it from somewhere else.
    ///
    /// This ID should be unique within all mappings in the compartment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
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
    pub on_activate: Option<LifecycleHook>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_deactivate: Option<LifecycleHook>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glue: Option<Glue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_audio_feedback: Option<SuccessAudioFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unprocessed: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleHook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_midi_feedback: Option<Vec<SendMidiFeedbackAction>>,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum SendMidiFeedbackAction {
    Raw { message: RawMidiMessage },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RawMidiMessage {
    HexString(String),
    ByteArray(Vec<u8>),
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum SuccessAudioFeedback {
    Simple,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ActivationCondition {
    Modifier(ModifierActivationCondition),
    Bank(BankActivationCondition),
    Eel(EelActivationCondition),
    Expression(ExpressionActivationCondition),
    TargetValue(TargetValueActivationCondition),
}

#[derive(Eq, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ModifierActivationCondition {
    pub modifiers: Option<Vec<ModifierState>>,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModifierState {
    pub parameter: ParamRef,
    pub on: bool,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BankActivationCondition {
    pub parameter: ParamRef,
    pub bank_index: u32,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EelActivationCondition {
    pub condition: String,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExpressionActivationCondition {
    pub condition: String,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TargetValueActivationCondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<String>,
    pub condition: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ParamRef {
    Index(u32),
    Key(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VirtualControlElementId {
    Indexed(u32),
    Named(String),
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum VirtualControlElementCharacter {
    Multi,
    Button,
}

impl Default for VirtualControlElementCharacter {
    fn default() -> Self {
        Self::Multi
    }
}

#[derive(Copy, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct OscArgument {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<OscArgKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_range: Option<Interval<f64>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
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
