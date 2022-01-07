mod clip;

pub use clip::*;
use reaper_high::{Project, Reaper};
use reaper_medium::PositionInSeconds;

mod slot;
pub use slot::*;

mod clip_source;

mod source_util;

pub fn get_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    let project = project.unwrap_or_else(|| Reaper::get().current_project());
    project.play_position_next_audio_block()
}
