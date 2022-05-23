use crate::persistence::session::Session;
use schemars::JsonSchema;

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct RealearnPersistenceRoot {
    _session: Session,
}
