use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(PartialEq, Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub index: u32,
    /// An optional key that you can assign to this parameter in order to refer
    /// to it from somewhere else.
    ///
    /// This key should be unique within this list of parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
