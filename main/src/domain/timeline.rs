use helgoboss_learn::BASE_EPSILON;
use reaper_high::{Project, Reaper};
use reaper_medium::{Bpm, Hz, MeasureMode, PositionInBeats, PositionInSeconds, ProjectContext};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub struct TimelineMoment {
    cursor_pos: PositionInSeconds,
    tempo: Bpm,
    next_bar: i32,
}

impl TimelineMoment {
    pub fn new(cursor_pos: PositionInSeconds, tempo: Bpm, next_bar: i32) -> Self {
        Self {
            next_bar,
            cursor_pos,
            tempo,
        }
    }

    pub fn cursor_pos(&self) -> PositionInSeconds {
        self.cursor_pos
    }

    pub fn tempo(&self) -> Bpm {
        self.tempo
    }

    pub fn next_bar(&self) -> i32 {
        self.next_bar
    }
}

pub struct ReaperProjectTimeline {
    project_context: ProjectContext,
}

impl ReaperProjectTimeline {
    pub fn new(project: Option<Project>) -> Self {
        Self {
            project_context: project
                .map(|p| p.context())
                .unwrap_or(ProjectContext::CurrentProject),
        }
    }
}

impl Timeline for ReaperProjectTimeline {
    fn cursor_pos(&self) -> PositionInSeconds {
        Reaper::get()
            .medium_reaper()
            .get_play_position_2_ex(self.project_context)
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        get_next_bar_at(timeline_pos, self.project_context)
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        get_pos_of_bar(bar, self.project_context)
    }

    fn is_running(&self) -> bool {
        let play_state = Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project_context);
        !play_state.is_paused
    }

    fn follows_reaper_transport(&self) -> bool {
        true
    }

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm {
        let play_state = Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project_context);
        // The idea is that we want to follow tempo envelopes while playing but not follow them
        // while paused (because we don't even see where the hypothetical play cursor is on the
        // timeline).
        let tempo_ref_pos = if play_state.is_playing || play_state.is_paused {
            timeline_pos
        } else {
            PositionInSeconds::new(0.0)
        };
        Reaper::get()
            .medium_reaper()
            .time_map_2_get_divided_bpm_at_time(self.project_context, tempo_ref_pos)
    }
}

pub trait Timeline {
    fn capture_moment(&self) -> TimelineMoment {
        let cursor_pos = self.cursor_pos();
        let tempo = self.tempo_at(cursor_pos);
        let next_bar = self.next_bar_at(cursor_pos);
        TimelineMoment::new(cursor_pos, tempo, next_bar)
    }

    fn cursor_pos(&self) -> PositionInSeconds;

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32;

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds;

    fn is_running(&self) -> bool;

    fn follows_reaper_transport(&self) -> bool;

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm;
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
    fn cursor_pos(&self) -> PositionInSeconds {
        PositionInSeconds::new(self.sample_count() as f64 / self.sample_rate().get())
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        // I guess an independent timeline shouldn't get this information from a project.
        // But let's see how to deal with that as soon as we put it to use.
        get_next_bar_at(timeline_pos, ProjectContext::CurrentProject)
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        // I guess an independent timeline shouldn't get this information from a project.
        // But let's see how to deal with that as soon as we put it to use.
        get_pos_of_bar(bar, ProjectContext::CurrentProject)
    }

    fn is_running(&self) -> bool {
        true
    }

    fn follows_reaper_transport(&self) -> bool {
        false
    }

    fn tempo_at(&self, _timeline_pos: PositionInSeconds) -> Bpm {
        Bpm::new(96.0)
    }
}

pub fn get_next_bar_at(cursor_pos: PositionInSeconds, proj_context: ProjectContext) -> i32 {
    let reaper = Reaper::get().medium_reaper();
    let res = reaper.time_map_2_time_to_beats(proj_context, cursor_pos);
    if res.beats_since_measure.get() <= BASE_EPSILON {
        res.measure_index
    } else {
        res.measure_index + 1
    }
}

pub fn get_pos_of_bar(bar: i32, proj_context: ProjectContext) -> PositionInSeconds {
    let reaper = Reaper::get().medium_reaper();
    reaper.time_map_2_beats_to_time(
        proj_context,
        MeasureMode::FromMeasureAtIndex(bar),
        PositionInBeats::new(0.0),
    )
}

impl<T: Timeline> Timeline for &T {
    fn capture_moment(&self) -> TimelineMoment {
        (*self).capture_moment()
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        (*self).cursor_pos()
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        (*self).next_bar_at(timeline_pos)
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        (*self).pos_of_bar(bar)
    }

    fn is_running(&self) -> bool {
        (*self).is_running()
    }

    fn follows_reaper_transport(&self) -> bool {
        (*self).follows_reaper_transport()
    }

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm {
        (*self).tempo_at(timeline_pos)
    }
}

static GLOBAL_STEADY_TIMELINE: SteadyTimeline = SteadyTimeline::new();

/// Returns a global timeline that is ever-increasing and not influenced by REAPER's transport.
pub fn global_steady_timeline() -> &'static SteadyTimeline {
    &GLOBAL_STEADY_TIMELINE
}
