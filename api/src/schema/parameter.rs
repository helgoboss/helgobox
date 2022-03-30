use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub index: u32,
    /// An optional ID that you can assign to this parameter in order to refer
    /// to it from somewhere else.
    ///
    /// This ID should be unique within all parameters in the same compartment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
}
