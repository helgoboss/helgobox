use crate::domain::Timeline;
use std::cmp;
use std::convert::TryInto;
use std::error::Error;
use std::ptr::null_mut;

use crate::domain::clip::source_util::pcm_source_is_midi;
use crate::domain::clip::{clip_timeline, clip_timeline_cursor_pos};
use helgoboss_learn::UnitValue;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use reaper_high::{Project, Reaper};
use reaper_medium::{
    BorrowedPcmSource, BorrowedPcmSourceTransfer, CustomPcmSource, DurationInBeats,
    DurationInSeconds, ExtendedArgs, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, MidiEvent,
    OwnedPcmSource, PcmSource, PeaksClearArgs, PositionInSeconds, PropertiesWindowArgs, ReaperStr,
    SaveStateArgs, SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
};

/// A PCM source which wraps a native REAPER PCM source and applies all kinds of clip
/// functionality to it.
///
/// For example, it makes sure it starts at the right position on the timeline.
///
/// It's intended to be continuously played by a preview register (immediately, unbuffered,
/// infinitely).
pub struct ClipPcmSource {
    /// Information about the wrapped source.
    inner: SourceInfo,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    /// This can change during the lifetime of this clip.
    repetition: Repetition,
    /// An ever-increasing counter which is used just for debugging purposes at the moment.
    counter: u64,
    /// The current state of this clip, containing only state which is non-derivable.
    state: ClipState,
}

struct SourceInfo {
    /// This source contains the actual audio/MIDI data.
    ///
    /// It doesn't change throughout the lifetime of this clip source, although I think it could.
    source: OwnedPcmSource,
    /// Caches the information if the inner clip source contains MIDI or audio material.
    is_midi: bool,
}

#[derive(Copy, Clone, Debug)]
enum Repetition {
    Infinitely,
    Times(u32),
}

impl Repetition {
    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Repetition::Infinitely
        } else {
            Repetition::Times(1)
        }
    }

    pub fn to_stop_instruction(self) -> Option<StopInstruction> {
        use Repetition::*;
        match self {
            Infinitely => None,
            Times(n) => Some(StopInstruction::AtEndOfCycle(cmp::max(n, 1) - 1)),
        }
    }
}

/// Represents a state of the clip wrapper PCM source.
#[derive(Copy, Clone, Debug)]
pub enum ClipState {
    Stopped,
    /// These phases are summarized because this distinction can be derived from the start
    /// position and the cursor position on the time line.
    ScheduledOrPlaying {
        play_info: PlayInfo,
        /// Only set if scheduled for stop.
        stop_pos: Option<StopInstruction>,
    },
    /// Very short transition state before entering another state.
    Suspending {
        reason: SuspensionReason,
        /// We still need the play info for fade out.
        play_info: PlayInfo,
        /// At which point on the timeline the transition started.
        transition_start_pos: PositionInSeconds,
    },
    Paused {
        /// Position within the clip at which should be resumed later.
        pos_within_clip: DurationInSeconds,
    },
}

impl ClipState {
    fn play_info(&self) -> Option<PlayInfo> {
        use ClipState::*;
        match self {
            Stopped | Paused { .. } => None,
            ScheduledOrPlaying { play_info, .. } | Suspending { play_info, .. } => Some(*play_info),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SuspensionReason {
    /// Play was suspended for initiating a retriggering, so the next state will be  
    /// [`ClipState::ScheduledOrPlaying`] again.
    Retrigger,
    /// Play was suspended for initiating a pause, so the next state will be [`ClipState::Paused`].
    Pause,
    /// Play was suspended for initiating a stop, so the next state will be [`ClipState::Stopped`].
    Stop,
    /// The clip might receive a play request when it's currently about to suspend due to pause,
    /// stop or retrigger. In this case it's important not to ignore the request because it can be
    /// annoying to have unfulfilled play requests. However, skipping the suspension and going
    /// straight to a playing state is not a good idea. We might get hanging notes. So we
    /// keep suspending but change the reason and thereby the next state (which will be
    /// [`ClipState::ScheduledOrPlaying`]).
    PlayWhileSuspending { start_pos: PositionInSeconds },
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ClipStopPosition {
    At(PositionInSeconds),
    AtEndOfClip,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum StopInstruction {
    At(PositionInSeconds),
    /// The first cycle is called cycle 0.
    AtEndOfCycle(u32),
}

impl ClipPcmSource {
    /// Wraps the given native REAPER PCM source.
    pub fn new(inner: OwnedPcmSource, project: Option<Project>) -> Self {
        let is_midi = pcm_source_is_midi(&inner);
        Self {
            inner: SourceInfo {
                source: inner,
                is_midi,
            },
            project,
            counter: 0,
            repetition: Repetition::Times(1),
            state: ClipState::Stopped,
        }
    }

    fn start_internal(
        &mut self,
        timeline_cursor_pos: PositionInSeconds,
        start_pos: PositionInSeconds,
        repeated: bool,
    ) {
        use ClipState::*;
        match self.state {
            // Not yet running.
            Stopped => self.schedule_start_internal(start_pos, repeated),
            ScheduledOrPlaying {
                stop_pos,
                play_info,
            } => {
                if stop_pos.is_some() {
                    // Playing already and scheduled for stop. Backpedal!
                    self.state = ClipState::ScheduledOrPlaying {
                        play_info,
                        stop_pos: None,
                    };
                } else {
                    // Scheduled for play or playing already.
                    let cursor_info = play_info.cursor_info_at(timeline_cursor_pos);
                    if cursor_info.has_started_already() {
                        // Already playing. Retrigger!
                        self.state = ClipState::Suspending {
                            reason: SuspensionReason::Retrigger,
                            play_info,
                            transition_start_pos: timeline_cursor_pos,
                        };
                    } else {
                        // Not yet playing. Reschedule!
                        self.schedule_start_internal(start_pos, repeated);
                    }
                }
            }
            Suspending {
                play_info,
                transition_start_pos,
                ..
            } => {
                // It's important to handle this, otherwise some play actions simply have no effect,
                // which is especially annoying when using transport sync because then it's like
                // forgetting that clip ... the next time the transport is stopped and started,
                // that clip won't play again.
                self.repetition = Repetition::from_bool(repeated);
                self.state = ClipState::Suspending {
                    reason: SuspensionReason::PlayWhileSuspending { start_pos },
                    play_info,
                    transition_start_pos,
                };
            }
            Paused { pos_within_clip } => {
                // Resume
                self.state = ClipState::ScheduledOrPlaying {
                    play_info: PlayInfo {
                        timeline_start_pos: timeline_cursor_pos,
                        clip_cursor_offset: pos_within_clip,
                    },
                    stop_pos: None,
                };
            }
        }
    }

    fn schedule_start_internal(&mut self, start_pos: PositionInSeconds, repeated: bool) {
        self.repetition = Repetition::from_bool(repeated);
        self.state = ClipState::ScheduledOrPlaying {
            play_info: PlayInfo {
                timeline_start_pos: start_pos,
                clip_cursor_offset: DurationInSeconds::ZERO,
            },
            stop_pos: None,
        };
    }

    fn create_cursor_and_length_info_at(
        &self,
        play_info: PlayInfo,
        timeline_cursor_pos: PositionInSeconds,
    ) -> CursorAndLengthInfo {
        let cursor_info = play_info.cursor_info_at(timeline_cursor_pos);
        self.create_cursor_and_length_info(cursor_info)
    }

    fn create_cursor_and_length_info(&self, cursor_info: CursorInfo) -> CursorAndLengthInfo {
        CursorAndLengthInfo {
            cursor_info,
            clip_length: self.inner_length(),
            repetition: self.repetition,
        }
    }

    /// Returns the position of the cursor on the parent timeline.
    ///
    /// When the project is not playing, it's a hypothetical position starting from the project
    /// play cursor position.
    fn timeline_cursor_pos(&self) -> PositionInSeconds {
        clip_timeline_cursor_pos(self.project)
    }

    fn get_suspension_follow_up_state(
        &self,
        reason: SuspensionReason,
        play_info: PlayInfo,
        timeline_cursor_pos: PositionInSeconds,
    ) -> ClipState {
        match reason {
            SuspensionReason::Retrigger => ClipState::ScheduledOrPlaying {
                play_info: PlayInfo {
                    timeline_start_pos: timeline_cursor_pos,
                    clip_cursor_offset: DurationInSeconds::ZERO,
                },
                stop_pos: None,
            },
            SuspensionReason::Pause => {
                let info = self.create_cursor_and_length_info_at(play_info, timeline_cursor_pos);
                let pos_within_clip = info
                    .hypothetical_pos_within_clip()
                    .try_into()
                    .unwrap_or_default();
                ClipState::Paused { pos_within_clip }
            }
            SuspensionReason::Stop => ClipState::Stopped,
            SuspensionReason::PlayWhileSuspending { start_pos } => ClipState::ScheduledOrPlaying {
                play_info: PlayInfo {
                    timeline_start_pos: start_pos,
                    clip_cursor_offset: DurationInSeconds::ZERO,
                },
                stop_pos: None,
            },
        }
    }

    fn fill_samples(
        &self,
        args: &mut GetSamplesArgs,
        info: CursorAndLengthInfo,
        clip_end_timeline_pos: Option<PositionInSeconds>,
    ) {
        // This means the clip is playing or about o play.
        // We want to start playing as soon as we reach the scheduled start position,
        // that means pos == 0.0. In order to do that, we need to take into account that
        // the audio buffer start point is not necessarily equal to the measure start
        // point. If we would naively start playing as soon as pos >= 0.0, we might skip
        // the first samples/messages! We need to start playing as soon as the end of
        // the audio block is located on or right to the scheduled start point
        // (end_pos >= 0.0).
        let desired_sample_count = args.block.length();
        let sample_rate = args.block.sample_rate().get();
        let block_duration = desired_sample_count as f64 / sample_rate;
        let start_pos_within_clip = info.hypothetical_pos_within_clip();
        let end_pos_within_clip = unsafe {
            PositionInSeconds::new_unchecked(start_pos_within_clip.get() + block_duration)
        };
        if end_pos_within_clip < PositionInSeconds::ZERO {
            // Complete block is located before start position
            return;
        }
        unsafe {
            if self.inner.is_midi {
                self.fill_samples_midi(start_pos_within_clip, args, &info);
            } else {
                self.fill_samples_audio(start_pos_within_clip, args, &info);
                self.post_process_audio(args, &info, clip_end_timeline_pos);
            }
        }
    }

    unsafe fn post_process_audio(
        &self,
        args: &mut GetSamplesArgs,
        info: &CursorAndLengthInfo,
        end_of_play_timeline_pos: Option<PositionInSeconds>,
    ) {
        // Parameters
        let start_pos = info.cursor_info.play_info.timeline_start_pos.get();
        let current_pos = info.cursor_info.timeline_cursor_pos.get();
        // TODO-high Implement fade at repetition. In future maybe even a crossfade (if enough
        //  audio material available at both sides).
        let end_pos = end_of_play_timeline_pos
            .map(|p| p.get())
            .unwrap_or(f64::MAX);
        let fade_length = click_prevention_fade_length().get();
        // Conversion to samples
        let sample_rate = args.block.sample_rate().get();
        let start_pos = (start_pos * sample_rate) as i64;
        let current_pos = (current_pos * sample_rate) as i64;
        let end_pos = (end_pos * sample_rate) as i64;
        let fade_length = (fade_length * sample_rate) as u64;
        // Processing
        let mut samples = args.block.samples_as_mut_slice();
        let length = args.block.length() as usize;
        let nch = args.block.nch() as usize;
        for frame in 0..length {
            let fade_factor = calculate_fade_factor_with_samples(
                start_pos,
                current_pos + frame as i64,
                end_pos,
                fade_length,
            );
            for ch in 0..nch {
                let sample = &mut samples[frame * nch + ch];
                *sample = *sample * fade_factor.get();
            }
        }
    }

    unsafe fn fill_samples_audio(
        &self,
        pos_within_clip: PositionInSeconds,
        args: &mut GetSamplesArgs,
        info: &CursorAndLengthInfo,
    ) {
        let desired_sample_count = args.block.length();
        let sample_rate = args.block.sample_rate().get();
        if pos_within_clip < PositionInSeconds::ZERO {
            // For audio, starting at a negative position leads to weird sounds.
            // That's why we need to query from 0.0 and
            // offset the provided sample buffer by that
            // amount.
            let sample_offset = (-pos_within_clip.get() * sample_rate) as i32;
            args.block.set_time_s(PositionInSeconds::ZERO);
            with_shifted_samples(args.block, sample_offset, |b| {
                self.inner.source.get_samples(b);
            });
        } else {
            args.block.set_time_s(pos_within_clip);
            self.inner.source.get_samples(args.block);
        }
        let written_sample_count = args.block.samples_out();
        if written_sample_count < desired_sample_count {
            // We have reached the end of the clip and it doesn't fill the
            // complete block.
            if info.is_last_cycle() {
                // Let preview register know that complete buffer has been
                // filled as desired in order to prevent retry (?) queries.
                args.block.set_samples_out(desired_sample_count);
            } else {
                // Repeat. Because we assume that the user cuts sources
                // sample-perfect, we must immediately fill the rest of the
                // buffer with the very
                // beginning of the source.
                // Audio. Start from zero and write just remaining samples.
                args.block.set_time_s(PositionInSeconds::ZERO);
                with_shifted_samples(args.block, written_sample_count, |b| {
                    self.inner.source.get_samples(b);
                });
                // Let preview register know that complete buffer has been filled.
                args.block.set_samples_out(desired_sample_count);
            }
        }
    }

    unsafe fn fill_samples_midi(
        &self,
        pos_within_clip: PositionInSeconds,
        args: &mut GetSamplesArgs,
        info: &CursorAndLengthInfo,
    ) {
        let desired_sample_count = args.block.length();
        let sample_rate = args.block.sample_rate().get();
        // For MIDI it seems to be okay to start at a negative position. The source
        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
        // offset.
        args.block.set_time_s(pos_within_clip);
        self.inner.source.get_samples(args.block);
        let written_sample_count = args.block.samples_out();
        if written_sample_count < desired_sample_count {
            // We have reached the end of the clip and it doesn't fill the
            // complete block.
            if info.is_last_cycle() {
                // Let preview register know that complete buffer has been
                // filled as desired in order to prevent retry (?) queries that
                // lead to double events.
                args.block.set_samples_out(desired_sample_count);
            } else {
                // Repeat. Fill rest of buffer with beginning of source.
                // We need to start from negative position so the frame
                // offset of the *added* MIDI events is correctly written.
                // The negative position should be as long as the duration of
                // samples already written.
                let written_duration = written_sample_count as f64 / sample_rate;
                let negative_pos = PositionInSeconds::new_unchecked(-written_duration);
                args.block.set_time_s(negative_pos);
                args.block.set_length(desired_sample_count);
                self.inner.source.get_samples(args.block);
            }
        }
    }
}

impl CustomPcmSource for ClipPcmSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        // Not correct but probably never used.
        self.inner.source.duplicate()
    }

    fn is_available(&mut self) -> bool {
        self.inner.source.is_available()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        self.inner.source.set_available(args.is_available);
    }

    fn get_type(&mut self) -> &ReaperStr {
        unsafe { self.inner.source.get_type_unchecked() }
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unsafe { self.inner.source.get_file_name_unchecked() }
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        self.inner.source.set_file_name(args.new_file_name)
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        self.inner.source.get_source()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        self.inner.source.set_source(args.source);
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.inner.source.get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        self.inner.source.get_sample_rate()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        // The clip source itself can be considered to represent an infinite-length "track".
        DurationInSeconds::MAX
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        let _ = self.inner.source.get_length_beats()?;
        Some(DurationInBeats::MAX)
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        self.inner.source.get_bits_per_sample()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        self.inner.source.get_preferred_position()
    }

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unsafe { self.inner.source.properties_window(args.parent_window) }
    }

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        // Debugging
        // if self.counter % 500 == 0 {
        //     let ptr = args.block.as_ptr();
        //     let raw = unsafe { ptr.as_ref() };
        //     dbg!(raw);
        // }
        let timeline = clip_timeline(self.project);
        if !timeline.is_running() {
            return;
        }
        self.counter += 1;
        use ClipState::*;
        match self.state {
            Stopped => {}
            Paused { .. } => {}
            Suspending {
                reason,
                play_info,
                transition_start_pos,
            } => {
                let timeline_cursor_pos = timeline.cursor_pos();
                if self.inner.is_midi {
                    // MIDI. Make everything get silent by sending the appropriate MIDI messages.
                    silence_midi(&args);
                    // Then immediately transition to the next state.
                    self.state =
                        self.get_suspension_follow_up_state(reason, play_info, timeline_cursor_pos);
                } else {
                    // Audio. Apply a small fadeout to prevent clicks.
                    let transition_end_pos = transition_start_pos + click_prevention_fade_length();
                    let info =
                        self.create_cursor_and_length_info_at(play_info, timeline_cursor_pos);
                    let transition_end_pos =
                        if let Some(eop_pos) = info.calc_end_of_play_timeline_pos(None) {
                            // Clip is about to end anyway. It's very unlikely but it could be
                            // that the natural end happens before the transition end. In this case,
                            // just start or continue applying the natural end fadeout.
                            cmp::min(transition_end_pos, eop_pos)
                        } else {
                            transition_end_pos
                        };
                    if timeline_cursor_pos < transition_end_pos {
                        self.fill_samples(&mut args, info, Some(transition_end_pos));
                    } else {
                        // Transition finished. Go to next state.
                        self.state = self.get_suspension_follow_up_state(
                            reason,
                            play_info,
                            timeline_cursor_pos,
                        );
                    }
                }
            }
            ScheduledOrPlaying {
                play_info,
                stop_pos,
            } => {
                let timeline_cursor_pos = timeline.cursor_pos();
                let info = self.create_cursor_and_length_info_at(play_info, timeline_cursor_pos);
                let end_of_play_timeline_pos = info.calc_end_of_play_timeline_pos(stop_pos);
                let is_within_play_bounds = end_of_play_timeline_pos
                    .map(|p| timeline_cursor_pos < p)
                    .unwrap_or(true);
                if is_within_play_bounds {
                    // There's still stuff to be played.
                    self.fill_samples(&mut args, info, end_of_play_timeline_pos);
                } else {
                    // We have reached the natural or scheduled end. Everything that needed to be
                    // played has been played in previous blocks. Audio fade outs have been applied
                    // as well, so no need to going to suspending state first. Go right to stop!
                    self.state = ClipState::Stopped;
                }
            }
        }
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unsafe {
            self.inner.source.get_peak_info(args.block);
        }
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unsafe {
            self.inner.source.save_state(args.context);
        }
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unsafe { self.inner.source.load_state(args.first_line, args.context) }
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
        self.inner.source.peaks_clear(args.delete_file);
    }

    fn peaks_build_begin(&mut self) -> bool {
        self.inner.source.peaks_build_begin()
    }

    fn peaks_build_run(&mut self) -> bool {
        self.inner.source.peaks_build_run()
    }

    fn peaks_build_finish(&mut self) {
        self.inner.source.peaks_build_finish();
    }

    unsafe fn extended(&mut self, args: ExtendedArgs) -> i32 {
        match args.call {
            EXT_QUERY_STATE => {
                *(args.parm_1 as *mut ClipState) = self.clip_state();
                1
            }
            EXT_SCHEDULE_START => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let pos: PositionInSeconds = *(args.parm_2 as *mut _);
                let repeated: bool = *(args.parm_3 as *mut _);
                self.schedule_start(timeline_cursor_pos, pos, repeated);
                1
            }
            EXT_START_IMMEDIATELY => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let repeated: bool = *(args.parm_2 as *mut _);
                self.start_immediately(timeline_cursor_pos, repeated);
                1
            }
            EXT_PAUSE => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.pause(timeline_cursor_pos);
                1
            }
            EXT_SCHEDULE_STOP => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let pos: ClipStopPosition = *(args.parm_2 as *mut _);
                self.schedule_stop(timeline_cursor_pos, pos);
                1
            }
            EXT_STOP_IMMEDIATELY => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.stop_immediately(timeline_cursor_pos);
                1
            }
            EXT_SEEK_TO => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                let pos: DurationInSeconds = *(args.parm_2 as *mut _);
                self.seek_to(timeline_cursor_pos, pos);
                1
            }
            EXT_QUERY_INNER_LENGTH => {
                *(args.parm_1 as *mut f64) = self.inner_length().get();
                1
            }
            EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                *(args.parm_2 as *mut Option<PositionInSeconds>) =
                    self.pos_within_clip(timeline_cursor_pos);
                1
            }
            EXT_QUERY_POS_FROM_START => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                *(args.parm_2 as *mut Option<PositionInSeconds>) =
                    self.pos_from_start(timeline_cursor_pos);
                1
            }
            EXT_ENABLE_REPEAT => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.set_repeated(timeline_cursor_pos, true);
                1
            }
            EXT_DISABLE_REPEAT => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.set_repeated(timeline_cursor_pos, false);
                1
            }
            _ => self
                .inner
                .source
                .extended(args.call, args.parm_1, args.parm_2, args.parm_3),
        }
    }
}

fn silence_midi(args: &GetSamplesArgs) {
    for ch in 0..16 {
        let all_notes_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_NOTES_OFF,
            U7::MIN,
        );
        let all_sound_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_SOUND_OFF,
            U7::MIN,
        );
        add_midi_event(args, all_notes_off);
        add_midi_event(args, all_sound_off);
    }
}

fn add_midi_event(args: &GetSamplesArgs, msg: RawShortMessage) {
    let mut event = MidiEvent::default();
    event.set_message(msg);
    args.block.midi_event_list().add_item(&event);
}

pub trait ClipPcmSourceSkills {
    /// Returns the state of this clip source.
    fn clip_state(&self) -> ClipState;

    /// Schedules clip playing.
    ///
    /// - Reschedules if not yet playing.
    /// - Stops and reschedules if already playing and not scheduled for stop.
    /// - Resumes immediately if paused (so the clip might out of sync!).
    /// - Backpedals if already playing and scheduled for stop.
    fn schedule_start(
        &mut self,
        timeline_cursor_pos: PositionInSeconds,
        pos: PositionInSeconds,
        repeated: bool,
    );

    /// Starts playback immediately.
    ///
    /// - Retriggers immediately if already playing and not scheduled for stop.
    /// - Resumes immediately if paused.
    /// - Backpedals if already playing and scheduled for stop.
    fn start_immediately(&mut self, timeline_cursor_pos: PositionInSeconds, repeated: bool);

    /// Pauses playback.
    fn pause(&mut self, timeline_cursor_pos: PositionInSeconds);

    /// Schedules clip stop.
    ///
    /// - Backpedals from scheduled start if not yet playing.
    /// - Stops immediately if paused.
    fn schedule_stop(&mut self, timeline_cursor_pos: PositionInSeconds, pos: ClipStopPosition);

    /// Stops playback immediately.
    ///
    /// - Backpedals from scheduled start if not yet playing.
    fn stop_immediately(&mut self, timeline_cursor_pos: PositionInSeconds);

    /// Seeks to the given position within the clip.
    ///
    /// This only has an effect if the clip is already and still playing.
    fn seek_to(&mut self, timeline_cursor_pos: PositionInSeconds, pos: DurationInSeconds);

    /// Returns the clip length.
    ///
    /// The clip length is different from the clip source length. The clip source length is infinite
    /// because it just acts as a sort of virtual track).
    fn inner_length(&self) -> DurationInSeconds;

    /// Changes whether to repeat or not repeat the clip.
    fn set_repeated(&mut self, timeline_cursor_pos: PositionInSeconds, repeated: bool);

    /// Returns the position within the clip.
    ///
    /// - Considers clip length.
    /// - Considers repeat.
    /// - Returns negative position if clip not yet playing.
    /// - Returns `None` if not scheduled, if single shot and reached end or if beyond scheduled
    /// stop or if clip length is zero.
    fn pos_within_clip(&self, timeline_cursor_pos: PositionInSeconds) -> Option<PositionInSeconds>;

    /// Returns the current position relative to the position at which the clip was scheduled for
    /// playing, taking the temporary offset into account (for seeking and continue-after-pause).
    ///
    /// - Returns a negative position if scheduled for play but not yet playing.
    /// - Neither the stop position nor the `repeated` field nor the clip length are considered
    ///   in this method! So it's just a hypothetical position that's intended for further analysis.
    fn pos_from_start(&self, timeline_cursor_pos: PositionInSeconds) -> Option<PositionInSeconds>;
}

impl ClipPcmSourceSkills for ClipPcmSource {
    fn clip_state(&self) -> ClipState {
        self.state
    }

    fn schedule_start(
        &mut self,
        timeline_cursor_pos: PositionInSeconds,
        pos: PositionInSeconds,
        repeated: bool,
    ) {
        self.start_internal(timeline_cursor_pos, pos, repeated);
    }

    fn start_immediately(&mut self, timeline_cursor_pos: PositionInSeconds, repeated: bool) {
        self.start_internal(timeline_cursor_pos, timeline_cursor_pos, repeated);
    }

    fn pause(&mut self, timeline_cursor_pos: PositionInSeconds) {
        use ClipState::*;
        match self.state {
            Stopped | Paused { .. } => {}
            ScheduledOrPlaying { play_info, .. } => {
                let info = play_info.cursor_info_at(timeline_cursor_pos);
                if info.has_started_already() {
                    // Playing. Pause!
                    // If this clip is scheduled for stop already, a pause will backpedal from
                    // that.
                    self.state = ClipState::Suspending {
                        reason: SuspensionReason::Pause,
                        play_info,
                        transition_start_pos: timeline_cursor_pos,
                    };
                } else {
                    // Not yet playing. Don't do anything at the moment.
                    // TODO-medium In future, we could take not an absolute start position but
                    //  a dynamic one (next bar, next beat, etc.) and then actually defer the
                    //  clip scheduling to the future. I think that would feel natural.
                }
            }
            Suspending {
                reason,
                play_info,
                transition_start_pos,
            } => {
                if reason != SuspensionReason::Pause {
                    // We are in another transition already. Simply change it to pause.
                    self.state = ClipState::Suspending {
                        reason: SuspensionReason::Pause,
                        play_info,
                        transition_start_pos,
                    };
                }
            }
        }
    }

    fn schedule_stop(&mut self, timeline_cursor_pos: PositionInSeconds, pos: ClipStopPosition) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            ScheduledOrPlaying {
                stop_pos,
                play_info,
            } => {
                if stop_pos.is_some() {
                    // Already scheduled for stop.
                    return;
                }
                let info = play_info.cursor_info_at(timeline_cursor_pos);
                self.state = if info.has_started_already() {
                    // Playing. Schedule stop.
                    let info = self.create_cursor_and_length_info(info);
                    if let Some(stop_pos) = info.determine_internal_stop_pos(pos) {
                        ClipState::ScheduledOrPlaying {
                            play_info,
                            stop_pos: Some(stop_pos),
                        }
                    } else {
                        // Looks like we were actually not playing after all.
                        ClipState::Stopped
                    }
                } else {
                    // Not yet playing. Backpedal.
                    ClipState::Stopped
                };
            }
            Paused { .. } => {
                self.state = ClipState::Stopped;
            }
            Suspending { .. } => {}
        }
    }

    fn stop_immediately(&mut self, timeline_cursor_pos: PositionInSeconds) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            ScheduledOrPlaying {
                stop_pos,
                play_info,
            } => {
                if stop_pos.is_some() {
                    // Scheduled for stop. Transition to stop now!
                    self.state = Suspending {
                        reason: SuspensionReason::Stop,
                        play_info,
                        transition_start_pos: timeline_cursor_pos,
                    };
                } else {
                    let info = play_info.cursor_info_at(timeline_cursor_pos);
                    self.state = if info.has_started_already() {
                        // Playing. Transition to stop.
                        Suspending {
                            reason: SuspensionReason::Stop,
                            play_info,
                            transition_start_pos: timeline_cursor_pos,
                        }
                    } else {
                        // Not yet playing. Backpedal.
                        ClipState::Stopped
                    };
                }
            }
            Suspending {
                reason,
                play_info,
                transition_start_pos,
            } => {
                if reason != SuspensionReason::Stop {
                    // We are in another transition already. Simply change it to stop.
                    self.state = Suspending {
                        reason: SuspensionReason::Stop,
                        play_info,
                        transition_start_pos,
                    };
                }
            }
            Paused { .. } => {
                self.state = ClipState::Stopped;
            }
        }
    }

    fn seek_to(&mut self, timeline_cursor_pos: PositionInSeconds, desired_pos: DurationInSeconds) {
        use ClipState::*;
        match self.state {
            Stopped | Suspending { .. } => {}
            ScheduledOrPlaying {
                play_info,
                stop_pos,
            } => {
                let info = self.create_cursor_and_length_info_at(play_info, timeline_cursor_pos);
                self.state = ClipState::ScheduledOrPlaying {
                    play_info: PlayInfo {
                        timeline_start_pos: play_info.timeline_start_pos,
                        clip_cursor_offset: info.calculate_clip_cursor_offset(desired_pos),
                    },
                    stop_pos,
                };
            }
            Paused { .. } => {
                self.state = Paused {
                    pos_within_clip: desired_pos,
                };
            }
        }
    }

    fn inner_length(&self) -> DurationInSeconds {
        self.inner.source.get_length().unwrap_or_default()
    }

    fn set_repeated(&mut self, timeline_cursor_pos: PositionInSeconds, repeated: bool) {
        self.repetition = {
            if repeated {
                Repetition::Infinitely
            } else {
                let times = if let Some(play_info) = self.state.play_info() {
                    let current_cycle_index = self
                        .create_cursor_and_length_info_at(play_info, timeline_cursor_pos)
                        .current_hypothetical_cycle_index();
                    if let Some(i) = current_cycle_index {
                        // Currently playing. Don't stop immediately but after current cycle!
                        i + 1
                    } else {
                        // Not playing yet.
                        1
                    }
                } else {
                    // If not playing, we want to stop after the 1st cycle in future.
                    1
                };
                Repetition::Times(times)
            }
        };
    }

    fn pos_within_clip(&self, timeline_cursor_pos: PositionInSeconds) -> Option<PositionInSeconds> {
        use ClipState::*;
        match self.state {
            Stopped => None,
            ScheduledOrPlaying { play_info, .. } | Suspending { play_info, .. } => {
                let info = self.create_cursor_and_length_info_at(play_info, timeline_cursor_pos);
                Some(info.hypothetical_pos_within_clip())
            }
            Paused { pos_within_clip } => Some(pos_within_clip.into()),
        }
    }

    fn pos_from_start(&self, timeline_cursor_pos: PositionInSeconds) -> Option<PositionInSeconds> {
        let play_info = self.state.play_info()?;
        Some(
            play_info
                .cursor_info_at(timeline_cursor_pos)
                .pos_from_start(),
        )
    }
}

impl ClipPcmSourceSkills for BorrowedPcmSource {
    fn clip_state(&self) -> ClipState {
        let mut state = ClipState::Stopped;
        unsafe {
            self.extended(
                EXT_QUERY_STATE,
                &mut state as *mut _ as _,
                null_mut(),
                null_mut(),
            )
        };
        state
    }

    fn schedule_start(
        &mut self,
        mut timeline_cursor_pos: PositionInSeconds,
        mut pos: PositionInSeconds,
        mut repeated: bool,
    ) {
        unsafe {
            self.extended(
                EXT_SCHEDULE_START,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut pos as *mut _ as _,
                &mut repeated as *mut _ as _,
            );
        }
    }

    fn start_immediately(
        &mut self,
        mut timeline_cursor_pos: PositionInSeconds,
        mut repeated: bool,
    ) {
        unsafe {
            self.extended(
                EXT_START_IMMEDIATELY,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut repeated as *mut _ as _,
                null_mut(),
            );
        }
    }

    fn schedule_stop(
        &mut self,
        mut timeline_cursor_pos: PositionInSeconds,
        mut pos: ClipStopPosition,
    ) {
        unsafe {
            self.extended(
                EXT_SCHEDULE_STOP,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut pos as *mut _ as _,
                null_mut(),
            );
        }
    }

    fn pause(&mut self, mut timeline_cursor_pos: PositionInSeconds) {
        unsafe {
            self.extended(
                EXT_PAUSE,
                &mut timeline_cursor_pos as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn stop_immediately(&mut self, mut timeline_cursor_pos: PositionInSeconds) {
        unsafe {
            self.extended(
                EXT_STOP_IMMEDIATELY,
                &mut timeline_cursor_pos as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn seek_to(&mut self, mut timeline_cursor_pos: PositionInSeconds, mut pos: DurationInSeconds) {
        unsafe {
            self.extended(
                EXT_SEEK_TO,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut pos as *mut _ as _,
                null_mut(),
            );
        }
    }

    fn inner_length(&self) -> DurationInSeconds {
        let mut l = 0.0;
        unsafe {
            self.extended(
                EXT_QUERY_INNER_LENGTH,
                &mut l as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        DurationInSeconds::new(l)
    }

    fn set_repeated(&mut self, mut timeline_cursor_pos: PositionInSeconds, repeated: bool) {
        let request = if repeated {
            EXT_ENABLE_REPEAT
        } else {
            EXT_DISABLE_REPEAT
        };
        unsafe {
            self.extended(
                request,
                &mut timeline_cursor_pos as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn pos_within_clip(
        &self,
        mut timeline_cursor_pos: PositionInSeconds,
    ) -> Option<PositionInSeconds> {
        let mut p: Option<PositionInSeconds> = None;
        unsafe {
            self.extended(
                EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut p as *mut _ as _,
                null_mut(),
            );
        }
        p
    }

    fn pos_from_start(
        &self,
        mut timeline_cursor_pos: PositionInSeconds,
    ) -> Option<PositionInSeconds> {
        let mut p: Option<PositionInSeconds> = None;
        unsafe {
            self.extended(
                EXT_QUERY_POS_FROM_START,
                &mut timeline_cursor_pos as *mut _ as _,
                &mut p as *mut _ as _,
                null_mut(),
            );
        }
        p
    }
}

unsafe fn with_shifted_samples(
    block: &mut BorrowedPcmSourceTransfer,
    offset: i32,
    f: impl FnOnce(&mut BorrowedPcmSourceTransfer),
) {
    // Shift samples.
    let original_length = block.length();
    let original_samples = block.samples();
    let shifted_samples = original_samples.offset((offset * block.nch()) as _);
    block.set_length(block.length() - offset);
    block.set_samples(shifted_samples);
    // Query inner source.
    f(block);
    // Unshift samples.
    block.set_length(original_length);
    block.set_samples(original_samples);
}

#[derive(Copy, Clone, Debug)]
pub struct PlayInfo {
    /// The timeline position on which the clip should start or started continuously playing.
    timeline_start_pos: PositionInSeconds,
    /// The cursor within the clip will be adjusted by this value.
    ///
    /// When playing, this is used for (temporary) seeking within the clip. For example, seeking
    /// forward is achieved by increasing the offset.
    ///
    /// When paused, this contains the position within the clip at the time the clip was paused.
    clip_cursor_offset: DurationInSeconds,
}

impl PlayInfo {
    /// Returns the start position on the timeline adjusted by the clip cursor offset which
    /// is used as offset for clip seeking and resume-after-pause.
    ///
    /// This method is suited for determining which part of the clip to play but not when
    /// the playback actually started.
    fn effective_start_pos(&self) -> PositionInSeconds {
        self.timeline_start_pos - self.clip_cursor_offset
    }

    fn cursor_info_at(&self, timeline_cursor_pos: PositionInSeconds) -> CursorInfo {
        CursorInfo {
            play_info: *self,
            timeline_cursor_pos,
        }
    }
}

/// Play info and current cursor position on the timeline.
struct CursorInfo {
    timeline_cursor_pos: PositionInSeconds,
    play_info: PlayInfo,
}

impl CursorInfo {
    fn has_started_already(&self) -> bool {
        self.pos_from_start() >= PositionInSeconds::ZERO
    }

    /// Returns the current position relative to the position at which the clip was scheduled for
    /// playing, taking the temporary offset into account (for seeking and continue-after-pause).
    ///
    /// - Returns a negative position if scheduled for play but not yet playing.
    /// - Neither the stop position nor the `repeated` field nor the clip length are considered
    ///   in this method! So it's just a hypothetical position that's intended for further analysis.
    /// - This can be used to determine when something is going to be playing or has started to
    ///   play, e.g. for fade application. For determining, which part of the clip is played,
    ///   the [`RunningClipState::clip_cursor_offset`] needs to be taken into account as well.
    fn pos_from_start(&self) -> PositionInSeconds {
        self.timeline_cursor_pos - self.play_info.effective_start_pos()
    }
}

struct CursorAndLengthInfo {
    cursor_info: CursorInfo,
    clip_length: DurationInSeconds,
    repetition: Repetition,
}

impl CursorAndLengthInfo {
    /// Returns the hypothetical position within the clip.
    ///
    /// - Considers clip length.
    /// - Returns position even if paused.
    /// - Returns negative position if clip not yet playing.
    pub fn hypothetical_pos_within_clip(&self) -> PositionInSeconds {
        let pos_from_start = self.cursor_info.pos_from_start();
        if pos_from_start < PositionInSeconds::ZERO {
            // Count-in phase. Report negative position.
            pos_from_start
        } else {
            // Playing.
            (pos_from_start % self.clip_length).unwrap_or_default()
        }
    }

    pub fn determine_internal_stop_pos(&self, pos: ClipStopPosition) -> Option<StopInstruction> {
        let internal_pos = match pos {
            ClipStopPosition::At(p) => StopInstruction::At(p),
            ClipStopPosition::AtEndOfClip => {
                let current_cycle = self.current_cycle_index()?;
                StopInstruction::AtEndOfCycle(current_cycle)
            }
        };
        Some(internal_pos)
    }

    /// Calculates the necessary clip cursor offset for making the clip play *NOW* at the given
    /// position within the clip.
    ///
    /// Should not be called in paused state (in which the `clip_cursor_offset` really just
    /// corresponds to the desired position within the clip).
    pub fn calculate_clip_cursor_offset(
        &self,
        desired_pos_within_clip: DurationInSeconds,
    ) -> DurationInSeconds {
        let timeline_cursor_pos = self.cursor_info.timeline_cursor_pos;
        let timeline_start_pos = self.cursor_info.play_info.timeline_start_pos;
        let timeline_target_pos =
            timeline_start_pos + desired_pos_within_clip - timeline_cursor_pos;
        timeline_target_pos.rem_euclid(self.clip_length)
    }

    /// Calculates in which cycle we are (starting with 0).
    ///
    /// Returns `None` if not yet playing or not repeated and clip length exceeded.
    fn current_cycle_index(&self) -> Option<u32> {
        let cycle_index = self.current_hypothetical_cycle_index()?;
        match self.repetition {
            Repetition::Infinitely => Some(cycle_index),
            Repetition::Times(n) => {
                if cycle_index < n {
                    Some(cycle_index)
                } else {
                    None
                }
            }
        }
    }

    fn is_last_cycle(&self) -> bool {
        match self.repetition {
            Repetition::Infinitely => false,
            Repetition::Times(n) => {
                if let Some(i) = self.current_hypothetical_cycle_index() {
                    i >= n - 1
                } else {
                    true
                }
            }
        }
    }

    /// Calculates in which cycle we are (starting with 0).
    ///
    /// Returns `None` if not yet playing.
    fn current_hypothetical_cycle_index(&self) -> Option<u32> {
        let pos_from_start = self.cursor_info.pos_from_start();
        if pos_from_start < PositionInSeconds::ZERO {
            // Not playing yet.
            None
        } else {
            Some((pos_from_start / self.clip_length).get() as u32)
        }
    }

    fn calc_end_of_play_timeline_pos(
        &self,
        scheduled_stop: Option<StopInstruction>,
    ) -> Option<PositionInSeconds> {
        let natural_stop = self.repetition.to_stop_instruction();
        IntoIterator::into_iter([natural_stop, scheduled_stop])
            .flatten()
            .map(|i| self.resolve_stop_instruction(i))
            .min()
    }

    fn resolve_stop_instruction(&self, stop_instruction: StopInstruction) -> PositionInSeconds {
        match stop_instruction {
            StopInstruction::At(pos) => pos,
            StopInstruction::AtEndOfCycle(n) => {
                self.cursor_info.play_info.effective_start_pos() + self.clip_length * (n + 1)
            }
        }
    }
}

// TODO-low Using this extended() mechanism is not very Rusty. The reason why we do it at the
//  moment is that we acquire access to the source by accessing the `source` attribute of the
//  preview register data structure. First, this can be *any* source in general, it's not
//  necessarily a PCM source for clips. Okay, this is not the primary issue. In practice we make
//  sure that it's only ever a PCM source for clips, so we could just do some explicit casting,
//  right? No. The thing which we get back there is not a reference to our ClipPcmSource struct.
//  It's the reaper-rs C++ PCM source, the one that delegates to our Rust struct. This C++ PCM
//  source implements the C++ virtual base class that REAPER API requires and it owns our Rust
//  struct. So if we really want to get rid of the extended() mechanism, we would have to access the
//  ClipPcmSource directly, without taking the C++ detour. And how is this possible in a safe Rusty
//  way that guarantees us that no one else is mutably accessing the source at the same time? By
//  wrapping the source in a mutex. However, this would mean that all calls to that source, even
//  the ones from REAPER would have to unlock the mutex first. For each source operation. That
//  sounds like a bad idea (or is it not because happy path is fast)? Well, but the point is, we
//  already have a mutex. The one around the preview register. This one is strictly necessary,
//  even the REAPER API requires it. As long as we have that outer mutex locked, we should in theory
//  be able to safely interact with our source directly from Rust. So in order to get rid of the
//  extended() mechanism, we would have to provide a way to get a correctly typed reference to our
//  original Rust struct. This itself is maybe possible by using some unsafe code, not sure.
const EXT_QUERY_STATE: i32 = 2359769;
const EXT_SCHEDULE_START: i32 = 2359771;
const EXT_QUERY_INNER_LENGTH: i32 = 2359772;
const EXT_ENABLE_REPEAT: i32 = 2359773;
const EXT_DISABLE_REPEAT: i32 = 2359774;
const EXT_QUERY_POS_WITHIN_CLIP_SCHEDULED: i32 = 2359775;
const EXT_SCHEDULE_STOP: i32 = 2359776;
const EXT_SEEK_TO: i32 = 2359778;
const EXT_STOP_IMMEDIATELY: i32 = 2359779;
const EXT_START_IMMEDIATELY: i32 = 2359781;
const EXT_QUERY_POS_FROM_START: i32 = 2359782;
const EXT_PAUSE: i32 = 2359783;

fn calculate_fade_factor_with_samples(
    start_pos: i64,
    current_pos: i64,
    end_pos: i64,
    fade_length: u64,
) -> UnitValue {
    let fade_length = fade_length as i64;
    let distance_from_start = current_pos - start_pos;
    let vol = if distance_from_start < 0 {
        // Not yet playing
        0.0
    } else if distance_from_start < fade_length {
        // Fade in
        distance_from_start as f64 / fade_length as f64
    } else {
        let distance_to_end = end_pos - current_pos;
        if distance_to_end > fade_length {
            // Playing
            1.0
        } else if distance_to_end < fade_length && distance_to_end > 0 {
            // Fade out
            distance_to_end as f64 / fade_length as f64
        } else {
            // Not playing anymore
            0.0
        }
    };
    UnitValue::new_clamped(vol)
}

fn click_prevention_fade_length() -> DurationInSeconds {
    DurationInSeconds::new(1.0)
}
