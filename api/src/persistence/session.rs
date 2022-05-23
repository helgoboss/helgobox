use crate::persistence::*;
use schemars::JsonSchema;

/// Only used for JSON schema generation at the moment.
#[derive(JsonSchema)]
pub struct Session {
    _main_compartment: Option<Compartment>,
    _clip_matrix: Option<Matrix>,
}
