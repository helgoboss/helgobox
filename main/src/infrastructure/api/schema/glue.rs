use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Default, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Glue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_interval: Option<(f64, f64)>,
}
