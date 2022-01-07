use helgoboss_learn::BASE_EPSILON;
use reaper_high::{Project, Reaper};
use reaper_medium::{Hz, MeasureMode, PositionInBeats, PositionInSeconds};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub struct TimelineMoment {
    cursor_pos: PositionInSeconds,
    next_bar_pos: PositionInSeconds,
}

impl TimelineMoment {
    pub fn new(cursor_pos: PositionInSeconds, next_bar_pos: PositionInSeconds) -> Self {
        Self {
            cursor_pos,
            next_bar_pos,
        }
    }

    pub fn cursor_pos(&self) -> PositionInSeconds {
        self.cursor_pos
    }
    pub fn next_bar_pos(&self) -> PositionInSeconds {
        self.next_bar_pos
    }
}

pub struct ReaperProjectTimeline {
    project: Option<Project>,
}

impl ReaperProjectTimeline {
    pub fn new(project: Option<Project>) -> Self {
        Self { project }
    }
}

impl ReaperProjectTimeline {
    fn project(&self) -> Project {
        self.project
            .unwrap_or_else(|| Reaper::get().current_project())
    }
}

impl Timeline for ReaperProjectTimeline {
    fn capture_moment(&self) -> TimelineMoment {
        let cursor_pos = self.cursor_pos();
        let next_bar_pos = get_next_bar_pos_from_project(cursor_pos, self.project());
        TimelineMoment::new(cursor_pos, next_bar_pos)
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        self.project().play_position_next_audio_block()
    }

    fn is_running(&self) -> bool {
        let play_state = Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project().context());
        !play_state.is_paused
    }

    fn follows_reaper_transport(&self) -> bool {
        true
    }
}

pub trait Timeline: Send + Sync {
    fn capture_moment(&self) -> TimelineMoment;

    fn cursor_pos(&self) -> PositionInSeconds;

    fn is_running(&self) -> bool;

    fn follows_reaper_transport(&self) -> bool;
}

pub struct SteadyTimeline {
    sample_counter: AtomicU64,
    sample_rate: AtomicU32,
}

impl SteadyTimeline {
    pub const fn new() -> Self {
        Self {
            sample_counter: AtomicU64::new(0),
            sample_rate: AtomicU32::new(0),
        }
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_counter.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> Hz {
        let discrete_sample_rate = self.sample_rate.load(Ordering::Relaxed) as f64;
        Hz::new(discrete_sample_rate)
    }

    pub fn advance_by(&self, buffer_length: u64, sample_rate: Hz) {
        self.sample_counter
            .fetch_add(buffer_length, Ordering::Relaxed);
        let discrete_sample_rate = sample_rate.get() as u32;
        self.sample_rate
            .store(discrete_sample_rate, Ordering::Relaxed);
    }
}

impl Timeline for SteadyTimeline {
    fn capture_moment(&self) -> TimelineMoment {
        let cursor_pos = self.cursor_pos();
        // I guess an independent timeline shouldn't get this information from a project.
        // But let's see how to deal with that as soon as we put it to use.
        let project = Reaper::get().current_project();
        let next_bar_pos = get_next_bar_pos_from_project(cursor_pos, project);
        TimelineMoment::new(cursor_pos, next_bar_pos)
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        PositionInSeconds::new(self.sample_count() as f64 / self.sample_rate().get())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn follows_reaper_transport(&self) -> bool {
        false
    }
}

pub fn get_next_bar_pos_from_project(
    cursor_pos: PositionInSeconds,
    project: Project,
) -> PositionInSeconds {
    let proj_context = project.context();
    let reaper = Reaper::get().medium_reaper();
    let res = reaper.time_map_2_time_to_beats(proj_context, cursor_pos);
    let next_measure_index = if res.beats_since_measure.get() <= BASE_EPSILON {
        res.measure_index
    } else {
        res.measure_index + 1
    };
    reaper.time_map_2_beats_to_time(
        proj_context,
        MeasureMode::FromMeasureAtIndex(next_measure_index),
        PositionInBeats::ZERO,
    )
}

impl<T: Timeline> Timeline for &T {
    fn capture_moment(&self) -> TimelineMoment {
        (*self).capture_moment()
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        (*self).cursor_pos()
    }

    fn is_running(&self) -> bool {
        (*self).is_running()
    }

    fn follows_reaper_transport(&self) -> bool {
        (*self).follows_reaper_transport()
    }
}

static GLOBAL_STEADY_TIMELINE: SteadyTimeline = SteadyTimeline::new();

/// Returns a global timeline that is ever-increasing and not influenced by REAPER's transport.
pub fn global_steady_timeline() -> &'static SteadyTimeline {
    &GLOBAL_STEADY_TIMELINE
}
