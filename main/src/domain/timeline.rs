use helgoboss_learn::BASE_EPSILON;
use reaper_high::{Project, Reaper};
use reaper_medium::{Bpm, Hz, MeasureMode, PositionInBeats, PositionInSeconds, ProjectContext};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub struct TimelineMoment {
    cursor_pos: PositionInSeconds,
    // TODO-high Mmh, wanted to remove this, but it could be nice to keep it for performance
    //  improvements. At the moment, each clip calculates the next bar pos itself from a common
    //  timeline position (so the result should be the same) but ideally, we would just have one
    //  timeline moment passing all of that data. Mmh.
    next_bar_pos: PositionInSeconds,
    tempo: Bpm,
}

impl TimelineMoment {
    pub fn new(cursor_pos: PositionInSeconds, next_bar_pos: PositionInSeconds, tempo: Bpm) -> Self {
        Self {
            cursor_pos,
            next_bar_pos,
            tempo,
        }
    }

    pub fn cursor_pos(&self) -> PositionInSeconds {
        self.cursor_pos
    }
    pub fn next_bar_pos(&self) -> PositionInSeconds {
        self.next_bar_pos
    }
    pub fn tempo(&self) -> Bpm {
        self.tempo
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

    fn next_bar_pos_at(&self, timeline_pos: PositionInSeconds) -> PositionInSeconds {
        get_next_bar_pos_from_project(timeline_pos, self.project_context)
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

    fn tempo(&self) -> Bpm {
        let play_state = Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project_context);
        // The idea is that we want to follow tempo envelopes while playing but not follow them
        // while paused (because we don't even see where the hypothetical play cursor is on the
        // timeline).
        let tempo_ref_pos = if play_state.is_playing || play_state.is_paused {
            self.cursor_pos()
        } else {
            PositionInSeconds::new(0.0)
        };
        self.tempo_at(tempo_ref_pos)
    }

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm {
        Reaper::get()
            .medium_reaper()
            .time_map_2_get_divided_bpm_at_time(self.project_context, timeline_pos)
    }
}

pub trait Timeline {
    fn capture_moment(&self) -> TimelineMoment {
        let cursor_pos = self.cursor_pos();
        let next_bar_pos = self.next_bar_pos();
        let tempo = self.tempo();
        TimelineMoment::new(cursor_pos, next_bar_pos, tempo)
    }

    fn cursor_pos(&self) -> PositionInSeconds;

    fn next_bar_pos(&self) -> PositionInSeconds {
        self.next_bar_pos_at(self.cursor_pos())
    }

    fn next_bar_pos_at(&self, timeline_pos: PositionInSeconds) -> PositionInSeconds;

    fn is_running(&self) -> bool;

    fn follows_reaper_transport(&self) -> bool;

    fn tempo(&self) -> Bpm {
        self.tempo_at(self.cursor_pos())
    }

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

    fn next_bar_pos_at(&self, timeline_pos: PositionInSeconds) -> PositionInSeconds {
        // I guess an independent timeline shouldn't get this information from a project.
        // But let's see how to deal with that as soon as we put it to use.
        get_next_bar_pos_from_project(timeline_pos, ProjectContext::CurrentProject)
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

pub fn get_next_bar_pos_from_project(
    cursor_pos: PositionInSeconds,
    proj_context: ProjectContext,
) -> PositionInSeconds {
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

    fn next_bar_pos(&self) -> PositionInSeconds {
        (*self).next_bar_pos()
    }

    fn next_bar_pos_at(&self, timeline_pos: PositionInSeconds) -> PositionInSeconds {
        (*self).next_bar_pos_at(timeline_pos)
    }

    fn is_running(&self) -> bool {
        (*self).is_running()
    }

    fn follows_reaper_transport(&self) -> bool {
        (*self).follows_reaper_transport()
    }

    fn tempo(&self) -> Bpm {
        (*self).tempo()
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
