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
pub struct SessionData {
    pub let_matched_events_through: bool,
    pub let_unmatched_events_through: bool,
    pub always_auto_detect_mode: bool,
    pub send_feedback_only_if_armed: bool,
}

impl SessionData {
    pub fn from_session(session: &Session) -> SessionData {
        SessionData {
            let_matched_events_through: session.let_matched_events_through.get(),
            let_unmatched_events_through: session.let_unmatched_events_through.get(),
            always_auto_detect_mode: session.always_auto_detect.get(),
            send_feedback_only_if_armed: session.send_feedback_only_if_armed.get(),
        }
    }

    pub fn apply_to_session(&self, session: &mut Session) -> Result<(), ValidationErrors> {
        session
            .let_matched_events_through
            .set(self.let_matched_events_through);
        session
            .let_unmatched_events_through
            .set(self.let_unmatched_events_through);
        session.always_auto_detect.set(self.always_auto_detect_mode);
        session
            .send_feedback_only_if_armed
            .set(self.send_feedback_only_if_armed);
        Ok(())
    }
}
