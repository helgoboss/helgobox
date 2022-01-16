mod clip;

pub use clip::*;
use reaper_high::{Project, Reaper};
use reaper_medium::{MeasureMode, PositionInBeats, PositionInSeconds};

mod slot;
use crate::domain::{
    get_next_bar_pos_from_project, global_steady_timeline, ReaperProjectTimeline, Timeline,
};
pub use clip_source::*;
pub use slot::*;

mod clip_source;

mod source_util;

mod time_stretcher;

mod buffer;

/// Delivers the timeline to be used for clips.
pub fn clip_timeline(project: Option<Project>) -> impl Timeline {
    // global_steady_timeline()
    ReaperProjectTimeline::new(project)
}

pub fn clip_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    clip_timeline(project).cursor_pos()
}
