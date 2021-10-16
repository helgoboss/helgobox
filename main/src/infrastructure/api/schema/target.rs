use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
pub struct Target {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<TargetUnit>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
pub enum TargetUnit {
    Native,
    Percent,
}
