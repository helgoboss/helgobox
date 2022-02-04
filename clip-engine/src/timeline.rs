use crate::{convert_duration_in_frames_to_seconds, convert_position_in_seconds_to_frames};
use atomic_float::AtomicF64;
use helgoboss_learn::BASE_EPSILON;
use reaper_high::{Project, Reaper};
use reaper_medium::{
    Bpm, Hz, MeasureMode, PlayState, PositionInBeats, PositionInSeconds, ProjectContext,
};
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

/// This represents the timeline of a REAPER project.
///
/// Characteristics:
///
/// - The cursor position (seconds) moves forward in real-time and independent from the current
///   tempo.
/// - If the project is paused, all positions freeze.
/// - The cursor position is reset whenever the user relocates the cursor in the project.
///     - This is okay for clip playing when the project is playing because in that case we want
///       to interrupt the clips and re-align to the changed situation.
///     - It's not okay for clip playing when the project is stopped because it would come as a
///       surprise for the user that clips are interrupted since they appear to be running
///       disconnected from the project timeline but actually aren't.
/// - If the project is playing (not stopped, not paused), the cursor position even resets when
///   changing the tempo. However, it leaves the bar/beat structure intact.
///     - The timeline cursor position doesn't influence the position within our clip, so we are
///       immune against these resets.
/// - If the project is not playing, tempo changes don't affect the cursor position but they reset
///   the bar/beat structure.
///     - It's understandable that the bar/beat structure is affected because REAPER has no tempo
///       envelope to look at, so it just does the most simple thing: Distributing the bars/beats
///       in a linear way.
///     - It's problematic for us that the bar/beat structure is affected because we use bars/beats
///       to keep the clips in sync (by scheduling them on start of bar). We need the bars/beats
///       structure to be fluent and adjust to tempo changes dynamically, much as a playing project
///       would do.
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

    fn is_playing_or_paused(&self) -> bool {
        let play_state = self.play_state();
        play_state.is_playing || play_state.is_paused
    }

    fn play_state(&self) -> PlayState {
        Reaper::get()
            .medium_reaper()
            .get_play_state_ex(self.project_context)
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
        !self.play_state().is_paused
    }

    fn follows_reaper_transport(&self) -> bool {
        true
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

/// This represents a self-made timeline that is driven by the global audio hook.
///
/// Characteristics:
///
/// - The cursor position (seconds) moves forward in real-time and independent from the current
///   tempo.
/// - The tempo is synchronized with the tempo of the current project.
// TODO-high If we really take this approach of the steady timeline for stopped projects, this
//  needs an overhaul: More AtomicF64, less conversion from/to frames.
pub struct SteadyTimeline {
    sample_counter: AtomicU64,
    sample_rate: AtomicU32,
    tempo: AtomicU64,
    bar_at_last_tempo_change: AtomicF64,
    sample_count_at_last_tempo_change: AtomicU64,
}

impl SteadyTimeline {
    pub const fn new() -> Self {
        Self {
            sample_counter: AtomicU64::new(0),
            sample_rate: AtomicU32::new(1),
            tempo: AtomicU64::new(1),
            bar_at_last_tempo_change: AtomicF64::new(0.0),
            sample_count_at_last_tempo_change: AtomicU64::new(0),
        }
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_counter.load(Ordering::SeqCst)
    }

    pub fn sample_rate(&self) -> Hz {
        let discrete_sample_rate = self.sample_rate.load(Ordering::SeqCst) as f64;
        Hz::new(discrete_sample_rate)
    }

    pub fn update(&self, buffer_length: u64, sample_rate: Hz, tempo: Bpm) {
        let prev_tempo = self.tempo();
        let prev_sample_count = self
            .sample_counter
            .fetch_add(buffer_length, Ordering::SeqCst);
        if tempo != prev_tempo {
            let prev_sample_count_at_last_tempo_change = self.sample_count_at_last_tempo_change();
            let prev_bar_at_last_tempo_change = self.bar_at_last_tempo_change();
            let prev_sample_rate = self.sample_rate();
            let bar = calc_bar_at(
                prev_sample_count,
                prev_sample_count_at_last_tempo_change,
                prev_bar_at_last_tempo_change,
                prev_tempo,
                prev_sample_rate,
            );
            self.sample_count_at_last_tempo_change
                .store(prev_sample_count, Ordering::SeqCst);
            self.bar_at_last_tempo_change.store(bar, Ordering::SeqCst);
        }
        self.tempo
            .store(tempo.get().round() as u64, Ordering::SeqCst);
        let discrete_sample_rate = sample_rate.get() as u32;
        self.sample_rate
            .store(discrete_sample_rate, Ordering::SeqCst);
    }

    fn tempo(&self) -> Bpm {
        Bpm::new(self.tempo.load(Ordering::SeqCst) as f64)
    }

    fn bar_at_last_tempo_change(&self) -> f64 {
        self.bar_at_last_tempo_change.load(Ordering::SeqCst)
    }

    fn sample_count_at_last_tempo_change(&self) -> u64 {
        self.sample_count_at_last_tempo_change
            .load(Ordering::SeqCst)
    }
}

fn calc_bar_at(
    sample_count: u64,
    sample_count_at_last_tempo_change: u64,
    bar_at_last_tempo_change: f64,
    current_tempo: Bpm,
    current_sample_rate: Hz,
) -> f64 {
    let beats_per_sec = current_tempo.get() / 60.0;
    // TODO-high Respect time signature.
    let bars_per_sec = beats_per_sec / 4.0;
    let samples_since_last_tempo_change = sample_count - sample_count_at_last_tempo_change;
    let secs_since_last_tempo_change = convert_duration_in_frames_to_seconds(
        samples_since_last_tempo_change as usize,
        current_sample_rate,
    );
    bar_at_last_tempo_change + secs_since_last_tempo_change.get() * bars_per_sec
}

fn calc_pos_of_bar(
    bar: i32,
    current_tempo: Bpm,
    bar_at_last_tempo_change: f64,
    sample_count_at_last_tempo_change: u64,
    current_sample_rate: Hz,
) -> PositionInSeconds {
    let beats_per_sec = current_tempo.get() / 60.0;
    // TODO-high Respect time signature.
    let bars_per_sec = beats_per_sec / 4.0;
    let secs_since_last_tempo_change = (bar as f64 - bar_at_last_tempo_change) / bars_per_sec;
    let secs_at_last_tempo_change = convert_duration_in_frames_to_seconds(
        sample_count_at_last_tempo_change as usize,
        current_sample_rate,
    );
    PositionInSeconds::new(secs_at_last_tempo_change.get() + secs_since_last_tempo_change)
}

impl Timeline for SteadyTimeline {
    fn cursor_pos(&self) -> PositionInSeconds {
        PositionInSeconds::new(self.sample_count() as f64 / self.sample_rate().get())
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        let sample_rate = self.sample_rate();
        let timeline_frame = convert_position_in_seconds_to_frames(timeline_pos, sample_rate);
        let bar = calc_bar_at(
            timeline_frame as u64,
            self.sample_count_at_last_tempo_change(),
            self.bar_at_last_tempo_change(),
            self.tempo(),
            sample_rate,
        );
        // TODO-high Use same epsilon fuzziness with REAPER project timeline
        (bar as i32) + 1
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        calc_pos_of_bar(
            bar,
            self.tempo(),
            self.bar_at_last_tempo_change(),
            self.sample_count_at_last_tempo_change(),
            self.sample_rate(),
        )
    }

    fn is_running(&self) -> bool {
        true
    }

    fn follows_reaper_transport(&self) -> bool {
        false
    }

    fn tempo_at(&self, _timeline_pos: PositionInSeconds) -> Bpm {
        self.tempo()
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

pub struct HybridTimeline {
    project_timeline: ReaperProjectTimeline,
    force_project_timeline: bool,
}

impl HybridTimeline {
    pub fn new(project: Option<Project>, force_project_timeline: bool) -> Self {
        Self {
            project_timeline: ReaperProjectTimeline::new(project),
            force_project_timeline,
        }
    }

    fn use_project_timeline(&self) -> bool {
        self.force_project_timeline || self.project_timeline.is_playing_or_paused()
    }
}

impl Timeline for HybridTimeline {
    fn cursor_pos(&self) -> PositionInSeconds {
        if self.use_project_timeline() {
            self.project_timeline.cursor_pos()
        } else {
            global_steady_timeline().cursor_pos()
        }
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        if self.use_project_timeline() {
            self.project_timeline.next_bar_at(timeline_pos)
        } else {
            global_steady_timeline().next_bar_at(timeline_pos)
        }
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        if self.use_project_timeline() {
            self.project_timeline.pos_of_bar(bar)
        } else {
            global_steady_timeline().pos_of_bar(bar)
        }
    }

    fn is_running(&self) -> bool {
        if self.use_project_timeline() {
            self.project_timeline.is_running()
        } else {
            global_steady_timeline().is_running()
        }
    }

    fn follows_reaper_transport(&self) -> bool {
        if self.use_project_timeline() {
            self.project_timeline.follows_reaper_transport()
        } else {
            global_steady_timeline().follows_reaper_transport()
        }
    }

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm {
        if self.use_project_timeline() {
            self.project_timeline.tempo_at(timeline_pos)
        } else {
            global_steady_timeline().tempo_at(timeline_pos)
        }
    }
}
