mod clip;

pub use clip::*;
use reaper_high::{Project, Reaper};
use reaper_medium::{MeasureMode, PositionInBeats, PositionInSeconds};

mod slot;
use crate::domain::{get_next_bar_at, global_steady_timeline, ReaperProjectTimeline, Timeline};
pub use clip_source::*;
pub use slot::*;

mod clip_source;

mod source_util;

mod buffer;

mod supplier;
pub use supplier::*;

/// Delivers the timeline to be used for clips.
pub fn clip_timeline(project: Option<Project>) -> impl Timeline {
    // TODO-high If we really choose this mix between steady and project timeline, we should build
    //  one timeline that exhibits different behavior depending on the play state.
    ReaperProjectTimeline::new(project)
    // global_steady_timeline()
}

pub fn clip_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    clip_timeline(project).cursor_pos()
}
