use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimeRuntimeRoot(
    QualifiedSlotEvent<OccasionalSlotUpdate>,
    QualifiedSlotEvent<FrequentSlotUpdate>,
);

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualifiedSlotEvent<T> {
    pub coordinates: SlotCoordinates,
    pub payload: T,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SlotCoordinates {
    pub column: u32,
    pub row: u32,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum OccasionalSlotUpdate {
    PlayState(ClipPlayStateUpdate),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum FrequentSlotUpdate {
    Position(ClipPositionUpdate),
    Peak(ClipPeakUpdate),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPlayStateUpdate {
    pub play_state: ClipPlayState,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPositionUpdate {
    pub position: f64,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ClipPeakUpdate {
    pub peak: f64,
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
