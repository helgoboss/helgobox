use schemars::JsonSchema;

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct RealearnRoot {
    _persistence: crate::persistence::RealearnPersistenceRoot,
    _runtime: crate::runtime::RealearnRuntimeRoot,
}
