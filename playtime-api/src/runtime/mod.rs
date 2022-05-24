use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimeRuntimeRoot {
    _qualified_clip_runtime_data_event: QualifiedClipRuntimeDataEvent,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualifiedClipRuntimeDataEvent {
    pub coordinates: SlotCoordinates,
    pub event: ClipRuntimeDataEvent,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SlotCoordinates {
    pub column: u32,
    pub row: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ClipRuntimeDataEvent {
    PlayState(ClipPlayStateEvent),
    Position(ClipPositionEvent),
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPlayStateEvent {
    pub play_state: ClipPlayState,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPositionEvent {
    pub position: f64,
}

/// Play state of a clip.
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
