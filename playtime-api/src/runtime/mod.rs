use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimeRuntimeRoot(ClipPlayState);

/// Play state of a clip.
///
/// Order of enum variants is important to aggregate the play state of multiple clips in one slot.
/// The slot always gets the "highest" play state.
// TODO-high-ms4 We don't need this in the API because we use gRPC for runtime stuff. Factor it back
//  into the clip engine module.
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Serialize, Deserialize, JsonSchema,
)]
pub enum ClipPlayState {
    Stopped,
    Ignited,
    Paused,
    ScheduledForPlayStart,
    ScheduledForPlayStop,
    ScheduledForRecordingStart,
    ScheduledForRecordingStop,
    Playing,
    Recording,
}
