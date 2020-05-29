use crate::domain::Session;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError, ValidationErrors};
use validator_derive::*;

/// This is the structure for loading and saving a ReaLearn session.
///
/// It's optimized for being represented as JSON. The JSON representation must be 100%
/// backward-compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
// #[validate(schema(function = "validate_schema"))]
pub struct SessionData {}

impl SessionData {
    pub fn from_session(session: &Session) -> SessionData {
        todo!()
    }

    pub fn apply_to_session(&self, session: &mut Session) -> Result<(), ValidationErrors> {
        todo!()
    }
}
