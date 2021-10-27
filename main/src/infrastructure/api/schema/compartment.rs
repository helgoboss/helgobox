use crate::infrastructure::api::schema::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Compartment {
    pub kind: CompartmentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_group: Option<Group>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<Group>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mappings: Option<Vec<Mapping>>,
}

#[derive(Copy, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub enum CompartmentKind {
    Controller,
    Main,
}
