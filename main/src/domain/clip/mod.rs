mod clip;

pub use clip::*;
use reaper_high::{Project, Reaper};
use reaper_medium::PositionInSeconds;

mod slot;
use crate::domain::{global_steady_timeline, Timeline};
pub use slot::*;

mod clip_source;

mod source_util;

/// Delivers the timeline to be used for clips.
pub fn clip_timeline(project: Option<Project>) -> impl Timeline {
    // global_steady_timeline()
    ReaperProjectTimeline::new(project.unwrap_or_else(|| Reaper::get().current_project()))
}

pub fn clip_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    clip_timeline(project).cursor_pos()
}

pub struct ReaperProjectTimeline {
    project: Project,
}

impl ReaperProjectTimeline {
    pub fn new(project: Project) -> Self {
        Self { project }
    }
}

impl Timeline for ReaperProjectTimeline {
    fn cursor_pos(&self) -> PositionInSeconds {
        self.project.play_position_next_audio_block()
    }

    fn is_running(&self) -> bool {
        let play_state = Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project.context());
        !play_state.is_paused
    }

    fn follows_reaper_transport(&self) -> bool {
        true
    }
}
