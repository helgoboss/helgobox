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

/// This represents the timeline of a REAPER project.
///
/// Characteristics:
///
/// - The cursor position (seconds) moves forward in real-time and independent from the current
///   tempo.
/// - If the project is paused, all positions freeze.
///   tempo, no matter if the project is playing or not.
/// - The cursor position is reset whenever the user relocates the cursor in the project.
///     - This is okay for clip playing when the project is playing because in that case we want
///       to interrupt the clips and re-align to the changed situation.
///     - It's not okay for clip playing when the project is paused because it would come as a
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

    fn rel_pos_from_bar(&self, timeline_pos: PositionInSeconds, bar: i32) -> PositionInSeconds {
        timeline_pos - get_pos_of_bar(bar, self.project_context)
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

    /// TODO-high Actually, the value returned here should not be interpreted as position in seconds
    ///  because it could have different meanings depending on the timeline. Its real meaning is to
    ///  represent an instant that lets you determine tempo and position of next bar. It could be
    ///  a frame, a second, a whatever. It shouldn't be interpreted by anything but the timeline
    ///  itself.
    fn cursor_pos(&self) -> PositionInSeconds;

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32;

    fn rel_pos_from_bar(&self, timeline_pos: PositionInSeconds, bar: i32) -> PositionInSeconds;

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
/// - TODO-high Make bars/beats structure not reset when changing tempo. Either by letting the
///    cursor position move forward tempo-dependent or by registering the last tempo change.
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
        self.sample_counter.load(Ordering::SeqCst)
    }

    pub fn sample_rate(&self) -> Hz {
        let discrete_sample_rate = self.sample_rate.load(Ordering::SeqCst) as f64;
        Hz::new(discrete_sample_rate)
    }

    pub fn update(&self, buffer_length: u64, sample_rate: Hz) {
        self.sample_counter
            .fetch_add(buffer_length, Ordering::SeqCst);
        let discrete_sample_rate = sample_rate.get() as u32;
        self.sample_rate
            .store(discrete_sample_rate, Ordering::SeqCst);
    }

    fn tempo(&self) -> Bpm {
        Reaper::get()
            .medium_reaper()
            .time_map_2_get_divided_bpm_at_time(
                ProjectContext::CurrentProject,
                PositionInSeconds::ZERO,
            )
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

    fn rel_pos_from_bar(&self, timeline_pos: PositionInSeconds, bar: i32) -> PositionInSeconds {
        let pos_of_bar = get_pos_of_bar(bar, ProjectContext::CurrentProject);
        timeline_pos - pos_of_bar
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

    fn rel_pos_from_bar(&self, timeline_pos: PositionInSeconds, bar: i32) -> PositionInSeconds {
        (*self).rel_pos_from_bar(timeline_pos, bar)
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
