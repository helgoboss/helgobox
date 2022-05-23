use schemars::JsonSchema;

/// Only used for JSON schema generation.
#[derive(JsonSchema)]
pub struct PlaytimeRuntimeRoot {
    _clip_play_state: ClipPlayState,
}

/// Play state of a clip.
#[derive(Copy, Clone, Eq, PartialEq, Debug, JsonSchema)]
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
