use schemars::JsonSchema;

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct RealearnRuntimeRoot {
    #[cfg(feature = "playtime")]
    _playtime_api: playtime_api::runtime::PlaytimeRuntimeRoot,
}
