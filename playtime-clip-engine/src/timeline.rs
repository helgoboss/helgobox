use crate::conversion_util::{
    convert_duration_in_frames_to_seconds, convert_position_in_seconds_to_frames,
};
use crate::ClipEngineResult;
use atomic::Atomic;
use helgoboss_learn::BASE_EPSILON;
use playtime_api::EvenQuantization;
use reaper_high::{Project, Reaper};
use reaper_medium::{
    Bpm, Hz, PlayState, PositionInQuarterNotes, PositionInSeconds, ProjectContext,
};
use static_assertions::const_assert;
use std::sync::atomic::{AtomicU64, Ordering};

/// Delivers the timeline to be used for clips.
pub fn clip_timeline(project: Option<Project>, force_reaper_timeline: bool) -> HybridTimeline {
    let reaper_timeline = ReaperTimeline::new(project);
    if force_reaper_timeline || reaper_timeline.is_playing_or_paused() {
        HybridTimeline::ReaperProject(reaper_timeline)
    } else {
        let steady_timeline = SteadyTimeline::new(global_steady_timeline_state());
        HybridTimeline::GlobalSteady(steady_timeline)
    }
}

pub fn clip_timeline_cursor_pos(project: Option<Project>) -> PositionInSeconds {
    clip_timeline(project, false).cursor_pos()
}

#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Debug)]
pub struct ReaperTimeline {
    project_context: ProjectContext,
}

impl ReaperTimeline {
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

impl Timeline for ReaperTimeline {
    fn cursor_pos(&self) -> PositionInSeconds {
        Reaper::get()
            .medium_reaper()
            .get_play_position_2_ex(self.project_context)
    }

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
    ) -> QuantizedPosition {
        // TODO-medium Handle in-measure tempo changes correctly (also for pos_of_quantized_pos).
        //  Time signature changes (always start a new measure) and on-measure tempo changes are
        //  handled correctly already.
        get_next_quantized_pos_at(timeline_pos, quantization, self.project_context)
    }

    fn pos_of_quantized_pos(&self, quantized_pos: QuantizedPosition) -> PositionInSeconds {
        get_pos_of_quantized_pos(quantized_pos, self.project_context)
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

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
    ) -> QuantizedPosition;

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        self.next_quantized_pos_at(timeline_pos, EvenQuantization::ONE_BAR)
            .position as _
    }

    fn pos_of_quantized_pos(&self, quantized_pos: QuantizedPosition) -> PositionInSeconds;

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        self.pos_of_quantized_pos(QuantizedPosition::bar(bar as _))
    }

    fn is_running(&self) -> bool;

    fn follows_reaper_transport(&self) -> bool;

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm;
}

/// Self-made timeline state that is driven by the global audio hook.
///
/// Characteristics:
///
/// - The cursor position (seconds) moves forward in real-time and independent from the current
///   tempo.
/// - The tempo is synchronized with the tempo of the current project at the current edit cursor
///   position.
/// - Uses the time signature of the current project at the current edit cursor position for
///   quantization purposes.
#[derive(Clone, Debug)]
pub struct SteadyTimeline<'a> {
    state: &'a SteadyTimelineState,
}

impl<'a> SteadyTimeline<'a> {
    pub fn new(state: &'a SteadyTimelineState) -> Self {
        Self { state }
    }

    fn time_signature_denominator(&self) -> u32 {
        // We could take the time signature from a particular project here (instead of from the
        // current project) but that wouldn't be consequent because the global
        // (project-independent) timeline state takes the tempo information always from the current
        // project.
        Reaper::get()
            .medium_reaper()
            .time_map_2_time_to_beats(
                ProjectContext::CurrentProject,
                SteadyTimelineState::tempo_and_time_sig_ref_pos(),
            )
            .time_signature
            .denominator
            .get()
    }
}

#[derive(Debug)]
pub struct SteadyTimelineState {
    sample_counter: AtomicU64,
    sample_rate: Atomic<Hz>,
    tempo: Atomic<Bpm>,
    beat_at_last_tempo_change: Atomic<f64>,
    sample_count_at_last_tempo_change: AtomicU64,
}

impl SteadyTimelineState {
    pub const fn new() -> Self {
        const_assert!(Atomic::<f64>::is_lock_free());
        Self {
            sample_counter: AtomicU64::new(0),
            sample_rate: Atomic::new(Hz::MIN),
            tempo: Atomic::new(Bpm::MIN),
            beat_at_last_tempo_change: Atomic::new(0.0),
            sample_count_at_last_tempo_change: AtomicU64::new(0),
        }
    }

    /// Supposed to be called once per audio callback.
    pub fn on_audio_buffer(&self, buffer_length: u64, sample_rate: Hz) {
        let tempo = Reaper::get()
            .medium_reaper()
            .time_map_2_get_divided_bpm_at_time(
                ProjectContext::CurrentProject,
                Self::tempo_and_time_sig_ref_pos(),
            );
        let prev_tempo = self.tempo();
        let prev_sample_count = self
            .sample_counter
            .fetch_add(buffer_length, Ordering::SeqCst);
        if tempo != prev_tempo {
            let prev_sample_count_at_last_tempo_change = self.sample_count_at_last_tempo_change();
            let prev_beat_at_last_tempo_change = self.beat_at_last_tempo_change();
            let prev_sample_rate = self.sample_rate();
            let beat = calc_beat_at(
                prev_sample_count,
                prev_sample_count_at_last_tempo_change,
                prev_beat_at_last_tempo_change,
                prev_tempo,
                prev_sample_rate,
            );
            self.sample_count_at_last_tempo_change
                .store(prev_sample_count, Ordering::SeqCst);
            self.beat_at_last_tempo_change.store(beat, Ordering::SeqCst);
        }
        self.tempo.store(tempo, Ordering::SeqCst);
        self.sample_rate.store(sample_rate, Ordering::SeqCst);
    }

    fn tempo_and_time_sig_ref_pos() -> PositionInSeconds {
        Reaper::get()
            .medium_reaper()
            .get_cursor_position_ex(ProjectContext::CurrentProject)
    }

    fn sample_count(&self) -> u64 {
        self.sample_counter.load(Ordering::SeqCst)
    }

    fn sample_rate(&self) -> Hz {
        self.sample_rate.load(Ordering::SeqCst)
    }

    fn tempo(&self) -> Bpm {
        self.tempo.load(Ordering::SeqCst)
    }

    fn beat_at_last_tempo_change(&self) -> f64 {
        self.beat_at_last_tempo_change.load(Ordering::SeqCst)
    }

    fn sample_count_at_last_tempo_change(&self) -> u64 {
        self.sample_count_at_last_tempo_change
            .load(Ordering::SeqCst)
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        PositionInSeconds::new(self.sample_count() as f64 / self.sample_rate().get())
    }

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
        time_sig_denominator: u32,
    ) -> QuantizedPosition {
        let sample_rate = self.sample_rate();
        let timeline_frame = convert_position_in_seconds_to_frames(timeline_pos, sample_rate);
        let sample_count_at_last_tempo_change = self.sample_count_at_last_tempo_change();
        let sample_count = timeline_frame as u64;
        if sample_count < sample_count_at_last_tempo_change {
            panic!("attempt to query next quantized position from a position in the past");
        }
        let accurate_beat = calc_beat_at(
            sample_count,
            sample_count_at_last_tempo_change,
            self.beat_at_last_tempo_change(),
            self.tempo(),
            sample_rate,
        );
        // The time signature denominator defines what one beat "means" (e.g. a quarter note).
        let ratio = quantization.denominator() as f64 / time_sig_denominator as f64;
        let accurate_pos = accurate_beat * ratio;
        calc_quantized_pos_from_accurate_pos(accurate_pos, quantization)
    }

    fn pos_of_quantized_pos(
        &self,
        quantized_pos: QuantizedPosition,
        time_sig_denominator: u32,
    ) -> PositionInSeconds {
        let ratio = time_sig_denominator as f64 / quantized_pos.denominator() as f64;
        let beat = quantized_pos.position() as f64 * ratio;
        calc_pos_of_beat(
            beat,
            self.tempo(),
            self.beat_at_last_tempo_change(),
            self.sample_count_at_last_tempo_change(),
            self.sample_rate(),
        )
    }
}

fn calc_beat_at(
    sample_count: u64,
    sample_count_at_last_tempo_change: u64,
    beat_at_last_tempo_change: f64,
    current_tempo: Bpm,
    current_sample_rate: Hz,
) -> f64 {
    debug_assert!(sample_count >= sample_count_at_last_tempo_change);
    let beats_per_sec = current_tempo.get() / 60.0;
    let samples_since_last_tempo_change = sample_count - sample_count_at_last_tempo_change;
    let secs_since_last_tempo_change = convert_duration_in_frames_to_seconds(
        samples_since_last_tempo_change as usize,
        current_sample_rate,
    );
    beat_at_last_tempo_change + secs_since_last_tempo_change.get() * beats_per_sec
}

fn calc_pos_of_beat(
    beat: f64,
    current_tempo: Bpm,
    beat_at_last_tempo_change: f64,
    sample_count_at_last_tempo_change: u64,
    current_sample_rate: Hz,
) -> PositionInSeconds {
    let beats_per_sec = current_tempo.get() / 60.0;
    let secs_since_last_tempo_change = (beat - beat_at_last_tempo_change) / beats_per_sec;
    let secs_at_last_tempo_change = convert_duration_in_frames_to_seconds(
        sample_count_at_last_tempo_change as usize,
        current_sample_rate,
    );
    PositionInSeconds::new(secs_at_last_tempo_change.get() + secs_since_last_tempo_change)
}

impl<'a> Timeline for SteadyTimeline<'a> {
    fn cursor_pos(&self) -> PositionInSeconds {
        self.state.cursor_pos()
    }

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
    ) -> QuantizedPosition {
        self.state.next_quantized_pos_at(
            timeline_pos,
            quantization,
            self.time_signature_denominator(),
        )
    }

    fn pos_of_quantized_pos(&self, quantized_pos: QuantizedPosition) -> PositionInSeconds {
        self.state
            .pos_of_quantized_pos(quantized_pos, self.time_signature_denominator())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn follows_reaper_transport(&self) -> bool {
        false
    }

    fn tempo_at(&self, _timeline_pos: PositionInSeconds) -> Bpm {
        self.state.tempo()
    }
}

fn get_next_quantized_pos_at(
    cursor_pos: PositionInSeconds,
    quantization: EvenQuantization,
    proj_context: ProjectContext,
) -> QuantizedPosition {
    let reaper = Reaper::get().medium_reaper();
    if quantization.denominator() == 1 {
        // We are looking for one of the next bars.
        let res = reaper.time_map_2_time_to_beats(proj_context, cursor_pos);
        let next_position = next_quantized_pos_sloppy(
            res.measure_index as i64,
            res.beats_since_measure.get(),
            quantization.numerator(),
        );
        QuantizedPosition::new(next_position, 1).unwrap()
    } else {
        // We are looking for the next fraction of a bar (e.g. the next 16th note).
        let qn = reaper.time_map_2_time_to_qn_abs(proj_context, cursor_pos);
        // Calculate ratio between our desired target unit (16th) and a quarter note (4th) = 4
        let ratio = quantization.denominator() as f64 / 4.0;
        // Current position in desired target unit (158.4 16th's).
        let accurate_pos = qn.get() * ratio;
        calc_quantized_pos_from_accurate_pos(accurate_pos, quantization)
    }
}

fn calc_quantized_pos_from_accurate_pos(
    accurate_pos: f64,
    quantization: EvenQuantization,
) -> QuantizedPosition {
    // Current position quantized (e.g. 158 16th's).
    let quantized_pos = accurate_pos.floor() as i64;
    // Difference (0.4 16th's).
    let within = accurate_pos - quantized_pos as f64;
    let next_position = next_quantized_pos_sloppy(quantized_pos, within, quantization.numerator());
    QuantizedPosition::new(next_position, quantization.denominator()).unwrap()
}

fn next_quantized_pos_sloppy(current_quantized_pos: i64, within: f64, numerator: u32) -> i64 {
    if within < BASE_EPSILON {
        // Just a tiny bit away from quantized position. Pretty sure the user meant to start now.
        return current_quantized_pos;
    }
    // Enough distance from quantized position.
    current_quantized_pos + numerator as i64
}

fn get_pos_of_quantized_pos(
    quantized_pos: QuantizedPosition,
    proj_context: ProjectContext,
) -> PositionInSeconds {
    let reaper = Reaper::get().medium_reaper();
    let qn = if quantized_pos.denominator() == 1 {
        // We are looking for the position of a bar.
        let res = reaper.time_map_get_measure_info(proj_context, quantized_pos.position as _);
        res.start_qn
    } else {
        // We are looking for the position of a fraction of a bar (e.g. a 16th note).
        // Calculate ratio between a quarter note (4th) and our desired target unit (16th) = 0.25
        let ratio = 4.0 / quantized_pos.denominator() as f64;
        PositionInQuarterNotes::new(quantized_pos.position as f64 * ratio)
    };
    reaper.time_map_2_qn_to_time_abs(proj_context, qn)
}

impl<T: Timeline> Timeline for &T {
    fn capture_moment(&self) -> TimelineMoment {
        (*self).capture_moment()
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        (*self).cursor_pos()
    }

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
    ) -> QuantizedPosition {
        (*self).next_quantized_pos_at(timeline_pos, quantization)
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        (*self).next_bar_at(timeline_pos)
    }

    fn pos_of_quantized_pos(&self, quantized_pos: QuantizedPosition) -> PositionInSeconds {
        (*self).pos_of_quantized_pos(quantized_pos)
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

static GLOBAL_STEADY_TIMELINE_STATE: SteadyTimelineState = SteadyTimelineState::new();

/// Returns the state for a global timeline that is ever-increasing and not influenced by REAPER's
/// transport.
pub fn global_steady_timeline_state() -> &'static SteadyTimelineState {
    &GLOBAL_STEADY_TIMELINE_STATE
}

#[derive(Clone, Debug)]
pub enum HybridTimeline {
    ReaperProject(ReaperTimeline),
    GlobalSteady(SteadyTimeline<'static>),
}

impl Timeline for HybridTimeline {
    fn capture_moment(&self) -> TimelineMoment {
        match self {
            HybridTimeline::ReaperProject(t) => t.capture_moment(),
            HybridTimeline::GlobalSteady(t) => t.capture_moment(),
        }
    }

    fn cursor_pos(&self) -> PositionInSeconds {
        match self {
            HybridTimeline::ReaperProject(t) => t.cursor_pos(),
            HybridTimeline::GlobalSteady(t) => t.cursor_pos(),
        }
    }

    fn next_quantized_pos_at(
        &self,
        timeline_pos: PositionInSeconds,
        quantization: EvenQuantization,
    ) -> QuantizedPosition {
        match self {
            HybridTimeline::ReaperProject(t) => t.next_quantized_pos_at(timeline_pos, quantization),
            HybridTimeline::GlobalSteady(t) => t.next_quantized_pos_at(timeline_pos, quantization),
        }
    }

    fn next_bar_at(&self, timeline_pos: PositionInSeconds) -> i32 {
        match self {
            HybridTimeline::ReaperProject(t) => t.next_bar_at(timeline_pos),
            HybridTimeline::GlobalSteady(t) => t.next_bar_at(timeline_pos),
        }
    }

    fn pos_of_quantized_pos(&self, quantized_pos: QuantizedPosition) -> PositionInSeconds {
        match self {
            HybridTimeline::ReaperProject(t) => t.pos_of_quantized_pos(quantized_pos),
            HybridTimeline::GlobalSteady(t) => t.pos_of_quantized_pos(quantized_pos),
        }
    }

    fn pos_of_bar(&self, bar: i32) -> PositionInSeconds {
        match self {
            HybridTimeline::ReaperProject(t) => t.pos_of_bar(bar),
            HybridTimeline::GlobalSteady(t) => t.pos_of_bar(bar),
        }
    }

    fn is_running(&self) -> bool {
        match self {
            HybridTimeline::ReaperProject(t) => t.is_running(),
            HybridTimeline::GlobalSteady(t) => t.is_running(),
        }
    }

    fn follows_reaper_transport(&self) -> bool {
        match self {
            HybridTimeline::ReaperProject(t) => t.follows_reaper_transport(),
            HybridTimeline::GlobalSteady(t) => t.follows_reaper_transport(),
        }
    }

    fn tempo_at(&self, timeline_pos: PositionInSeconds) -> Bpm {
        match self {
            HybridTimeline::ReaperProject(t) => t.tempo_at(timeline_pos),
            HybridTimeline::GlobalSteady(t) => t.tempo_at(timeline_pos),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct QuantizedPosition {
    position: i64,
    denominator: u32,
}

impl QuantizedPosition {
    pub fn bar(position: i64) -> Self {
        Self {
            position,
            denominator: 1,
        }
    }

    pub fn new(position: i64, denominator: u32) -> ClipEngineResult<Self> {
        if denominator == 0 {
            return Err("denominator must be > 0");
        }
        let p = Self {
            position,
            denominator,
        };
        Ok(p)
    }

    pub fn from_quantization(
        quantization: EvenQuantization,
        timeline: &HybridTimeline,
        ref_pos: Option<PositionInSeconds>,
    ) -> Self {
        let ref_pos = ref_pos.unwrap_or_else(|| timeline.cursor_pos());
        timeline.next_quantized_pos_at(ref_pos, quantization)
    }

    /// The position, that is the number of intervals from timeline zero.
    pub fn position(&self) -> i64 {
        self.position
    }

    /// The quotient that divides the bar into multiple equally-sized portions.
    ///
    /// E.g. 16 if it's a sixteenth note or 1 if it's a whole bar.
    pub fn denominator(&self) -> u32 {
        self.denominator
    }
}
