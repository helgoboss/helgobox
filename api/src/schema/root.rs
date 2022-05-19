use crate::schema::session::Session;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only used for JSON schema generation.
#[derive(Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReaLearn {
    session: Session,
}
