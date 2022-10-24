use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimeRuntimeRoot(ClipPlayState);

/// Play state of a clip.
// TODO-high-clip-matrix We don't need this in the API because we use gRPC for runtime stuff. Factor it back
//  into the clip engine module.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub enum ClipPlayState {
    Stopped,
    ScheduledForPlayStart,
    Playing,
    Paused,
    ScheduledForPlayStop,
    ScheduledForRecordingStart,
    Recording,
    ScheduledForRecordingStop,
}
