mod matrix;
pub use matrix::*;

mod timeline;
pub use timeline::*;

mod clip;
pub use clip::*;

use reaper_high::{Project, Reaper};
use reaper_medium::{MeasureMode, PositionInBeats, PositionInSeconds};

mod clip_content;
pub use clip_content::*;

mod source_util;

mod buffer;
pub use buffer::*;

mod supplier;
pub use supplier::*;

mod column;
pub use column::*;

mod column_source;
pub use column_source::*;

mod new_slot;
pub use new_slot::*;

mod new_clip;
pub use new_clip::*;

mod tempo_util;

/// Delivers the timeline to be used for clips.
pub fn clip_timeline(project: Option<Project>, force_project_timeline: bool) -> impl Timeline {
    HybridTimeline::new(project, force_project_timeline)
}

pub fn clip_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    clip_timeline(project, false).cursor_pos()
}

pub type ClipEngineResult<T> = Result<T, &'static str>;
