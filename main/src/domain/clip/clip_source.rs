// TODO-medium Give this file a last overhaul as soon as things run as they should. There were many
//  changes and things might not be implemented/named optimally. Position naming is very
//  inconsistent at the moment.
use std::cmp;
use std::convert::TryInto;
use std::error::Error;
use std::ptr::null_mut;

use crate::domain::clip::source_util::pcm_source_is_midi;
use crate::domain::clip::{clip_timeline, clip_timeline_cursor_pos};
use crate::domain::Timeline;
use helgoboss_learn::UnitValue;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use reaper_high::{Project, Reaper};
use reaper_medium::{
    BorrowedPcmSource, BorrowedPcmSourceTransfer, Bpm, CustomPcmSource, DurationInBeats,
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
    inner: InnerSource,
    /// Should be set to the project of the ReaLearn instance or `None` if on monitoring FX.
    project: Option<Project>,
    /// This can change during the lifetime of this clip.
    repetition: Repetition,
    /// Changes the tempo of this clip in addition to the natural tempo change.
    manual_tempo_factor: f64,
    /// An ever-increasing counter which is used just for debugging purposes at the moment.
    debug_counter: u64,
    /// The current state of this clip, containing only state which is non-derivable.
    state: ClipState,
}

struct InnerSource {
    /// This source contains the actual audio/MIDI data.
    ///
    /// It doesn't change throughout the lifetime of this clip source, although I think it could.
    source: OwnedPcmSource,
    /// Caches the information if the inner clip source contains MIDI or audio material.
    is_midi: bool,
}

impl InnerSource {
    fn original_tempo(&self) -> Bpm {
        // TODO-high Correctly determine: For audio, guess depending on length or read metadata or
        //  let overwrite by user.
        // For MIDI, an arbitrary but constant value is enough!
        Bpm::new(96.0)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Repetition {
    Infinitely,
    Once,
}

impl Repetition {
    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Repetition::Infinitely
        } else {
            Repetition::Once
        }
    }
}

/// Represents a state of the clip wrapper PCM source.
#[derive(Copy, Clone, Debug)]
pub enum ClipState {
    /// At this state, the clip is stopped. No fade-in, no fade-out ... nothing.
    ///
    /// The player can stop in this state.
    Stopped,
    ScheduledOrPlaying {
        play_instruction: PlayInstruction,
        /// Set as soon as the actual start has been resolved (from the play time field).
        resolved_play_data: Option<ResolvedPlayData>,
        scheduled_for_stop: bool,
    },
    /// Very short transition for fade outs or sending all-notes-off before entering another state.
    Suspending {
        reason: SuspensionReason,
        /// We still need the play info for fade out.
        play_info: ResolvedPlayData,
        transition_countdown: DurationInSeconds,
    },
    /// At this state, the clip is paused. No fade-in, no fade-out ... nothing.
    ///
    /// The player can stop in this state.
    Paused {
        /// Position *within* the clip at which should be resumed later.
        next_block_pos: DurationInSeconds,
    },
}

impl ClipState {
    fn play_info(&self) -> Option<ResolvedPlayData> {
        use ClipState::*;
        match self {
            ScheduledOrPlaying {
                resolved_play_data: Some(play_info),
                ..
            }
            | Suspending { play_info, .. } => Some(*play_info),
            _ => None,
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
    PlayWhileSuspending { play_time: ClipPlayTime },
}

#[derive(Clone, Copy)]
pub struct PlayArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub play_time: ClipPlayTime,
    pub repetition: Repetition,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PlayInstruction {
    /// We consider the absolute scheduled play position as part of the instruction. It's important
    /// not to resolve it super-late because then each clip calculates its own positions, which
    /// can be bad when starting multiple clips at once, e.g. synced to REAPER transport.
    pub scheduled_play_pos: PositionInSeconds,
    pub initial_tempo: Bpm,
}

impl PlayInstruction {
    fn from_play_time(
        play_time: ClipPlayTime,
        timeline_cursor_pos: PositionInSeconds,
        timeline: impl Timeline,
    ) -> Self {
        use ClipPlayTime::*;
        let scheduled_play_pos = match play_time {
            Immediately => timeline_cursor_pos,
            NextBar => timeline.next_bar_pos_at(timeline_cursor_pos),
        };
        Self {
            scheduled_play_pos,
            initial_tempo: timeline.tempo_at(timeline_cursor_pos),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ClipPlayTime {
    Immediately,
    NextBar,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ClipStopTime {
    Immediately,
    EndOfClip,
}

impl ClipPcmSource {
    /// Wraps the given native REAPER PCM source.
    pub fn new(inner: OwnedPcmSource, project: Option<Project>) -> Self {
        let is_midi = pcm_source_is_midi(&inner);
        Self {
            inner: InnerSource {
                source: inner,
                is_midi,
            },
            project,
            debug_counter: 0,
            repetition: Repetition::Once,
            state: ClipState::Stopped,
            manual_tempo_factor: 1.0,
        }
    }

    fn countdown_to_end_of_clip(&self, pos_within_clip: PositionInSeconds) -> DurationInSeconds {
        let duration = self.native_clip_length().get() - pos_within_clip.get();
        if duration < 0.0 {
            DurationInSeconds::ZERO
        } else {
            DurationInSeconds::new(duration)
        }
    }

    fn calc_final_tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        let timeline_tempo_factor = timeline_tempo.get() / self.inner.original_tempo().get();
        // (self.manual_tempo_factor * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
        // TODO-medium Enable manual tempo factor at some point when everything is working.
        //  At the moment this introduces too many uncertainties and false positive bugs.
        (1.0 * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
    }

    fn schedule_play_internal(&mut self, args: PlayArgs) {
        self.repetition = args.repetition;
        self.state = ClipState::ScheduledOrPlaying {
            play_instruction: PlayInstruction::from_play_time(
                args.play_time,
                args.timeline_cursor_pos,
                self.timeline(),
            ),
            resolved_play_data: None,
            scheduled_for_stop: false,
        };
    }

    fn create_cursor_and_length_info_at(
        &self,
        play_info: ResolvedPlayData,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
    ) -> CursorAndLengthInfo {
        let cursor_info = play_info.cursor_info_at(timeline_cursor_pos);
        self.create_cursor_and_length_info(cursor_info, timeline_tempo)
    }

    fn create_cursor_and_length_info(
        &self,
        cursor_info: CursorInfo,
        timeline_tempo: Bpm,
    ) -> CursorAndLengthInfo {
        CursorAndLengthInfo {
            cursor_info,
            clip_length: self.clip_length(timeline_tempo),
            repetition: self.repetition,
        }
    }

    /// Returns the parent timeline.
    fn timeline(&self) -> impl Timeline {
        clip_timeline(self.project)
    }

    fn get_samples_internal(
        &mut self,
        args: &mut GetSamplesArgs,
        timeline: impl Timeline,
        timeline_cursor_pos: PositionInSeconds,
    ) {
        let timeline_tempo = timeline.tempo();
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        use ClipState::*;
        match self.state {
            Stopped => {}
            Paused { .. } => {}
            Suspending {
                reason,
                play_info,
                transition_countdown,
            } => {
                if self.inner.is_midi {
                    // MIDI. Make everything get silent by sending the appropriate MIDI messages.
                    silence_midi(&args);
                    // Then immediately transition to the next state.
                    self.state = self.get_suspension_follow_up_state(
                        reason,
                        play_info,
                        timeline_cursor_pos,
                        &timeline,
                    );
                } else {
                    // Audio. Apply a small fadeout to prevent clicks.
                    let cursor_and_length_info = self.create_cursor_and_length_info_at(
                        play_info,
                        timeline_cursor_pos,
                        timeline_tempo,
                    );
                    let block_info = BlockInfo::new(
                        args.block,
                        cursor_and_length_info,
                        final_tempo_factor,
                        Some(transition_countdown),
                    );
                    self.fill_samples(args, &block_info);
                    // We want the fade to always have the same length, no matter the tempo.
                    let next_transition_countdown =
                        transition_countdown.get() - block_info.duration().get();
                    self.state = if next_transition_countdown > 0.0 {
                        // Transition ongoing
                        Suspending {
                            reason,
                            play_info,
                            transition_countdown: DurationInSeconds::new(next_transition_countdown),
                        }
                    } else {
                        // Transition finished. Go to next state.
                        self.get_suspension_follow_up_state(
                            reason,
                            play_info,
                            timeline_cursor_pos,
                            &timeline,
                        )
                    };
                }
            }
            ScheduledOrPlaying {
                play_instruction,
                resolved_play_data: play_info,
                scheduled_for_stop,
            } => {
                // Resolve play info if not yet resolved.
                let play_info = play_info.unwrap_or_else(|| {
                    // So, this is how we do play scheduling. Whenever the preview register
                    // calls get_samples() and we are in a fresh ScheduledOrPlaying state, the
                    // relative count-in time will be determined. Based on the given absolute
                    // scheduled-play position. 1. We use a *relative* count-in (instead of just
                    // using the absolute scheduled-play position and check if we reached it)
                    // in order to respect arbitrary tempo changes during the count-in phase and
                    // still end up starting on the correct point in time. 2. We resolve the
                    // count-in length here in the real-time context, not before! In particular not
                    // at the time the play is requested. At that time we just calculate the
                    // absolute position. Reason: The timeline_cursor_pos at play-request time
                    // is not necessarily the same as the timeline_cursor_pos at which the
                    // preview register "picks up" our new play state in get_samples(). If it's not,
                    // we would start advancing the count-in cursor from a wrong initial state
                    // and therefore end up with the wrong point in time for starting the clip
                    // (too late, to be accurate, because we would start advancing too late).
                    let data = ResolvedPlayData {
                        next_block_pos: timeline_cursor_pos - play_instruction.scheduled_play_pos,
                    };
                    dbg!(data);
                    data
                });
                let cursor_and_length_info = self.create_cursor_and_length_info_at(
                    play_info,
                    timeline_cursor_pos,
                    timeline_tempo,
                );
                // This will contain the remaining seconds to stop (always a positive number).
                let stop_at_end_of_clip = scheduled_for_stop || self.repetition == Repetition::Once;
                let stop_countdown = if stop_at_end_of_clip {
                    Some(self.countdown_to_end_of_clip(play_info.next_block_pos))
                } else {
                    None
                };
                let block_info = BlockInfo::new(
                    args.block,
                    cursor_and_length_info,
                    final_tempo_factor,
                    stop_countdown,
                );
                self.fill_samples(args, &block_info);
                // This is the point where we advance the block position.
                let next_play_info = ResolvedPlayData {
                    next_block_pos: {
                        // TODO-medium This mechanism of advancing the position on every call by
                        //  the block duration relies on the fact that the preview
                        //  register timeline calls us continuously and never twice per block.
                        //  It would be better not to make that assumption and make this more
                        //  stable by actually looking at the diff between the currently requested
                        //  time_s and the previously requested time_s. If this diff is zero or
                        //  doesn't correspond to the non-tempo-adjusted block duration, we know
                        //  something is wrong.
                        let block_end_pos = block_info.tempo_adjusted_block_end_pos();
                        if block_end_pos < PositionInSeconds::ZERO {
                            // This is still a *pure* count-in. No modulo logic yet.
                            // Also, we don't advance the position by a block duration that is
                            // adjusted using our normal tempo factor because at the time the
                            // initial countdown value was resolved, REAPER already took the current
                            // tempo into account. However, we must calculate a new tempo factor
                            //  based on possible tempo changes during the count-in phase!
                            // TODO-high When transport is not playing and we change the cursor
                            //  position, new count-ins in relation to the already playing clips
                            //  change. I think because the project timeline resets whenever we
                            //  change the cursor position, which makes the next-bar calculation
                            //  using a different origin. Crap.
                            let tempo_factor =
                                timeline_tempo.get() / play_instruction.initial_tempo.get();
                            let duration = (block_info.duration() * tempo_factor)
                                .expect("tempo factor never negative");
                            block_info.block_start_pos() + duration
                        } else {
                            // Playing already.
                            // Here we make sure that we always stay within the borders of the inner
                            // source. We don't use every-increasing positions because then tempo
                            // changes are not smooth anymore in subsequent cycles.
                            PositionInSeconds::new(
                                block_end_pos.get() % self.native_clip_length().get(),
                            )
                        }
                    },
                };
                self.state = if stop_countdown
                    .map(|c| c > block_info.tempo_adjusted_duration())
                    .unwrap_or(true)
                {
                    // There's still something to play.
                    ScheduledOrPlaying {
                        play_instruction,
                        resolved_play_data: Some(next_play_info),
                        scheduled_for_stop,
                    }
                } else {
                    // We have reached the natural or scheduled end. Everything that needed to be
                    // played has been played in previous blocks. Audio fade outs have been applied
                    // as well, so no need to going to suspending state first. Go right to stop!
                    Stopped
                };
            }
        }
    }

    fn get_suspension_follow_up_state(
        &self,
        reason: SuspensionReason,
        play_info: ResolvedPlayData,
        timeline_cursor_pos: PositionInSeconds,
        timeline: impl Timeline,
    ) -> ClipState {
        match reason {
            SuspensionReason::Retrigger => ClipState::ScheduledOrPlaying {
                play_instruction: PlayInstruction::from_play_time(
                    ClipPlayTime::Immediately,
                    timeline_cursor_pos,
                    timeline,
                ),
                resolved_play_data: None,
                scheduled_for_stop: false,
            },
            SuspensionReason::Pause => ClipState::Paused {
                next_block_pos: play_info
                    .next_block_pos
                    .try_into()
                    .unwrap_or(DurationInSeconds::ZERO),
            },
            SuspensionReason::Stop => ClipState::Stopped,
            SuspensionReason::PlayWhileSuspending { play_time } => ClipState::ScheduledOrPlaying {
                play_instruction: PlayInstruction::from_play_time(
                    play_time,
                    timeline_cursor_pos,
                    timeline,
                ),
                resolved_play_data: None,
                scheduled_for_stop: false,
            },
        }
    }

    fn fill_samples(&mut self, args: &mut GetSamplesArgs, info: &BlockInfo) {
        // This means the clip is playing or about o play.
        // We want to start playing as soon as we reach the scheduled start position,
        // that means pos == 0.0. In order to do that, we need to take into account that
        // the audio buffer start point is not necessarily equal to the measure start
        // point. If we would naively start playing as soon as pos >= 0.0, we might skip
        // the first samples/messages! We need to start playing as soon as the end of
        // the audio block is located on or right to the scheduled start point
        // (end_pos >= 0.0).
        if info.tempo_adjusted_block_end_pos() < PositionInSeconds::ZERO {
            // Complete block is located before start position (pure count-in block).
            return;
        }
        // At this point we are sure that the end of the block is right of the start position. The
        // start of the block might still be left of the start position (negative number).
        unsafe {
            if self.inner.is_midi {
                self.fill_samples_midi(args, &info);
            } else {
                self.fill_samples_audio(args, &info);
                // TODO-high Reenable post-processing
                // self.post_process_audio(args, &info, stop_countdown);
            }
        }
    }

    unsafe fn fill_samples_audio(&self, args: &mut GetSamplesArgs, info: &BlockInfo) {
        let outer_sample_rate = info.sample_rate();
        // TODO-medium We shouldn't modify the existing block but create a new one. Otherwise
        //  it's hard to keep track which changes concern the inner source and which one this
        //  source.
        args.block
            .set_sample_rate(info.tempo_adjusted_sample_rate());
        if info.block_start_pos() < PositionInSeconds::ZERO {
            dbg!("Audio starting at negative position");
            // For audio, starting at a negative position leads to weird sounds.
            // That's why we need to query from 0.0 and offset the resulting sample buffer by that
            // amount. We calculate the sample offset with the outer sample rate because this
            // doesn't concern the inner source content.
            let sample_offset = (-info.block_start_pos().get() * outer_sample_rate.get()) as i32;
            args.block.set_time_s(PositionInSeconds::ZERO);
            with_shifted_samples(args.block, sample_offset, |b| {
                // TODO-high Buffering
                self.inner.source.get_samples(b);
            });
        } else {
            args.block.set_time_s(info.block_start_pos());
            // TODO-high Buffering
            self.inner.source.get_samples(args.block);
        }
        let written_sample_count = args.block.samples_out();
        if written_sample_count < info.length() as _ {
            // We have reached the end of the clip and it doesn't fill the complete block.
            if info.is_last_block() {
                dbg!("Audio end of last cycle");
                // Let preview register know that complete buffer has been
                // filled as desired in order to prevent retry (?) queries.
                args.block.set_samples_out(info.length() as _);
            } else {
                dbg!("Audio repeat");
                // Repeat. Because we assume that the user cuts sources
                // sample-perfect, we must immediately fill the rest of the
                // buffer with the very beginning of the source. Start from zero and write just
                // remaining samples.
                args.block.set_time_s(PositionInSeconds::ZERO);
                with_shifted_samples(args.block, written_sample_count, |b| {
                    // TODO-high Buffering
                    self.inner.source.get_samples(b);
                });
                // Let preview register know that complete buffer has been filled.
                args.block.set_samples_out(info.length() as _);
            }
        }
    }

    unsafe fn fill_samples_midi(&self, args: &mut GetSamplesArgs, info: &BlockInfo) {
        // Force MIDI tempo, then *we* can deal with on-the-fly tempo changes that occur while
        // playing instead of REAPER letting use its generic mechanism that leads to duplicate
        // notes, probably through internal position changes.
        // TODO-high This only prevents duplicate notes when increasing tempo, not when decreasing
        //  it. Not sure what's still interfering.
        args.block.set_force_bpm(self.inner.original_tempo());
        let outer_sample_rate = info.sample_rate();
        // For MIDI it seems to be okay to start at a negative position. The source
        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
        // offset.
        args.block.set_time_s(info.block_start_pos());
        args.block
            .set_sample_rate(info.tempo_adjusted_sample_rate());
        self.inner.source.get_samples(args.block);
        let written_sample_count = args.block.samples_out();
        if written_sample_count < info.length() as _ {
            // We have reached the end of the clip and it doesn't fill the
            // complete block.
            if info.is_last_block() {
                dbg!("MIDI end of last cycle");
                // Let preview register know that complete buffer has been
                // filled as desired in order to prevent retry (?) queries that
                // lead to double events.
                args.block.set_samples_out(info.length as _);
            } else {
                dbg!("MIDI repeat");
                // Repeat. Fill rest of buffer with beginning of source.
                // We need to start from negative position so the frame
                // offset of the *added* MIDI events is correctly written.
                // The negative position should be as long as the duration of
                // samples already written.
                let written_duration = written_sample_count as f64 / outer_sample_rate.get();
                let negative_pos =
                    PositionInSeconds::new_unchecked(-written_duration) * info.final_tempo_factor();
                args.block.set_time_s(negative_pos);
                args.block.set_length(info.length as _);
                self.inner.source.get_samples(args.block);
            }
        }
    }

    // unsafe fn post_process_audio(
    //     &self,
    //     args: &mut GetSamplesArgs,
    //     info: &CursorAndLengthInfo,
    //     stop_countdown: Option<DurationInSeconds>,
    // ) {
    //     // Parameters in seconds
    //     let timeline_start_pos = info.cursor_info.play_info.timeline_start_pos.get();
    //     let rel_block_start_pos = info.cursor_info.play_info.next_block_pos;
    //     let rel_stop_pos = stop_pos
    //         .map(|p| p.get() - timeline_start_pos)
    //         .unwrap_or(f64::MAX);
    //     let clip_cursor_offset = info.cursor_info.play_info.clip_cursor_offset.get();
    //     // Conversion to samples
    //     let sample_rate = args.block.sample_rate().get();
    //     let calc = FadeCalculator {
    //         end_pos: (rel_stop_pos * sample_rate) as u64,
    //         clip_cursor_offset: (clip_cursor_offset * sample_rate) as u64,
    //         clip_length: (info.clip_length.get() * sample_rate) as u64,
    //         start_end_fade_length: (start_end_fade_length().get() * sample_rate) as u64,
    //         intermediate_fade_length: (repetition_fade_length().get() * sample_rate) as u64,
    //     };
    //     let block_pos = (rel_block_start_pos * sample_rate) as i64;
    //     // Processing
    //     let mut samples = args.block.samples_as_mut_slice();
    //     let length = args.block.length() as usize;
    //     let nch = args.block.nch() as usize;
    //     for frame in 0..length {
    //         let fade_factor = calc.calculate_fade_factor(block_pos + frame as i64);
    //         for ch in 0..nch {
    //             let sample = &mut samples[frame * nch + ch];
    //             *sample = *sample * fade_factor;
    //         }
    //     }
    // }
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
        // TODO-medium assert_no_alloc when the time has come.
        // Make sure that in any case, we are only queried once per time, without retries.
        unsafe {
            args.block.set_samples_out(args.block.length());
        }
        // Get main timeline info
        let timeline = self.timeline();
        if !timeline.is_running() {
            // Main timeline is paused. Don't play, we don't want to play the same buffer
            // repeatedly!
            // TODO-high Pausing main transport and continuing has timing issues.
            return;
        }
        let timeline_cursor_pos = timeline.cursor_pos();
        // Get samples
        self.get_samples_internal(&mut args, timeline, timeline_cursor_pos);
        debug_assert_eq!(args.block.samples_out(), args.block.length());
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
            EXT_CLIP_STATE => {
                *(args.parm_1 as *mut ClipState) = self.clip_state();
                1
            }
            EXT_PLAY => {
                let inner_args = *(args.parm_1 as *mut _);
                self.play(inner_args);
                1
            }
            EXT_PAUSE => {
                let timeline_cursor_pos: PositionInSeconds = *(args.parm_1 as *mut _);
                self.pause(timeline_cursor_pos);
                1
            }
            EXT_STOP => {
                let inner_args = *(args.parm_1 as *mut _);
                self.stop(inner_args);
                1
            }
            EXT_SEEK_TO => {
                let inner_args = *(args.parm_1 as *mut _);
                self.seek_to(inner_args);
                1
            }
            EXT_CLIP_LENGTH => {
                let timeline_tempo: Bpm = *(args.parm_1 as *mut _);
                *(args.parm_2 as *mut DurationInSeconds) = self.clip_length(timeline_tempo);
                1
            }
            EXT_NATIVE_CLIP_LENGTH => {
                *(args.parm_1 as *mut DurationInSeconds) = self.native_clip_length();
                1
            }
            EXT_POS_WITHIN_CLIP => {
                let inner_args: PosWithinClipArgs = *(args.parm_1 as *mut _);
                *(args.parm_2 as *mut Option<PositionInSeconds>) = self.pos_within_clip(inner_args);
                1
            }
            EXT_PROPORTIONAL_POS_WITHIN_CLIP => {
                let inner_args = *(args.parm_1 as *mut _);
                *(args.parm_2 as *mut Option<UnitValue>) =
                    self.proportional_pos_within_clip(inner_args);
                1
            }
            EXT_SET_TEMPO_FACTOR => {
                let tempo_factor: f64 = *(args.parm_1 as *mut _);
                self.set_tempo_factor(tempo_factor);
                1
            }
            EXT_TEMPO_FACTOR => {
                *(args.parm_1 as *mut f64) = self.get_tempo_factor();
                1
            }
            EXT_SET_REPEATED => {
                let inner_args = *(args.parm_1 as *mut _);
                self.set_repeated(inner_args);
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

    /// Starts or schedules clip playing.
    ///
    /// - Reschedules if not yet playing.
    /// - Retriggers/reschedules if already playing and not scheduled for stop.
    /// - Resumes immediately if paused (so the clip might out of sync!).
    /// - Backpedals if already playing and scheduled for stop.
    fn play(&mut self, args: PlayArgs);

    /// Pauses playback.
    fn pause(&mut self, timeline_cursor_pos: PositionInSeconds);

    /// Stops the clip or schedules the stop.
    ///
    /// - Backpedals from scheduled start if not yet playing.
    /// - Stops immediately if paused.
    fn stop(&mut self, args: StopArgs);

    /// Seeks to the given position within the clip.
    ///
    /// This only has an effect if the clip is already and still playing.
    fn seek_to(&mut self, args: SeekToArgs);

    /// Returns the clip length.
    ///
    /// The clip length is different from the clip source length. The clip source length is infinite
    /// because it just acts as a sort of virtual track).
    fn clip_length(&self, timeline_tempo: Bpm) -> DurationInSeconds;

    /// Returns the original length of the clip, tempo-independent.
    fn native_clip_length(&self) -> DurationInSeconds;

    /// Manually adjusts the play tempo by the given factor (in addition to the automatic
    /// timeline tempo adjustment).
    fn set_tempo_factor(&mut self, tempo_factor: f64);

    /// Returns the tempo factor.
    fn get_tempo_factor(&self) -> f64;

    /// Changes whether to repeat or not repeat the clip.
    fn set_repeated(&mut self, args: SetRepeatedArgs);

    /// Returns the position within the clip.
    ///
    /// - Considers clip length.
    /// - Considers repeat.
    /// - Returns negative position if clip not yet playing.
    /// - Returns `None` if not scheduled, if single shot and reached end or if beyond scheduled
    /// stop or if clip length is zero.
    fn pos_within_clip(&self, args: PosWithinClipArgs) -> Option<PositionInSeconds>;

    /// Returns the position within the clip as proportional value.
    fn proportional_pos_within_clip(&self, args: PosWithinClipArgs) -> Option<UnitValue>;
}

impl ClipPcmSourceSkills for ClipPcmSource {
    fn clip_state(&self) -> ClipState {
        self.state
    }

    fn play(&mut self, args: PlayArgs) {
        use ClipState::*;
        match self.state {
            // Not yet running.
            Stopped => self.schedule_play_internal(args),
            ScheduledOrPlaying {
                play_instruction: play_time,
                scheduled_for_stop,
                resolved_play_data: play_info,
            } => {
                if scheduled_for_stop {
                    // Scheduled for stop. Backpedal!
                    self.state = ClipState::ScheduledOrPlaying {
                        play_instruction: play_time,
                        resolved_play_data: play_info,
                        scheduled_for_stop: false,
                    };
                } else {
                    // Scheduled for play or playing already.
                    if let Some(play_info) = play_info {
                        let info = play_info.cursor_info_at(args.timeline_cursor_pos);
                        if info.has_started_already() {
                            // Already playing. Retrigger!
                            self.state = ClipState::Suspending {
                                reason: SuspensionReason::Retrigger,
                                play_info,
                                transition_countdown: start_end_fade_length(),
                            };
                        } else {
                            // Not yet playing. Reschedule!
                            self.schedule_play_internal(args);
                        }
                    } else {
                        // Not yet playing. Reschedule!
                        self.schedule_play_internal(args);
                    }
                }
            }
            Suspending {
                play_info,
                transition_countdown,
                ..
            } => {
                // It's important to handle this, otherwise some play actions simply have no effect,
                // which is especially annoying when using transport sync because then it's like
                // forgetting that clip ... the next time the transport is stopped and started,
                // that clip won't play again.
                self.repetition = args.repetition;
                self.state = ClipState::Suspending {
                    reason: SuspensionReason::PlayWhileSuspending {
                        play_time: args.play_time,
                    },
                    play_info,
                    transition_countdown,
                };
            }
            Paused { next_block_pos } => {
                // Resume
                self.state = ClipState::ScheduledOrPlaying {
                    play_instruction: PlayInstruction::from_play_time(
                        args.play_time,
                        args.timeline_cursor_pos,
                        self.timeline(),
                    ),
                    resolved_play_data: None,
                    scheduled_for_stop: false,
                };
            }
        }
    }

    fn pause(&mut self, timeline_cursor_pos: PositionInSeconds) {
        use ClipState::*;
        match self.state {
            Stopped | Paused { .. } => {}
            ScheduledOrPlaying {
                resolved_play_data: play_info,
                ..
            } => {
                if let Some(play_info) = play_info {
                    let info = play_info.cursor_info_at(timeline_cursor_pos);
                    if info.has_started_already() {
                        // Playing. Pause!
                        // (If this clip is scheduled for stop already, a pause will backpedal from
                        // that.)
                        self.state = ClipState::Suspending {
                            reason: SuspensionReason::Pause,
                            play_info,
                            transition_countdown: start_end_fade_length(),
                        };
                    }
                }
                // If not yet playing, we don't do anything at the moment.
                // TODO-medium In future, we could defer the clip scheduling to the future. I think
                //  that would feel natural.
            }
            Suspending {
                reason,
                play_info,
                transition_countdown,
            } => {
                if reason != SuspensionReason::Pause {
                    // We are in another transition already. Simply change it to pause.
                    self.state = ClipState::Suspending {
                        reason: SuspensionReason::Pause,
                        play_info,
                        transition_countdown,
                    };
                }
            }
        }
    }

    fn stop(&mut self, args: StopArgs) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            ScheduledOrPlaying {
                play_instruction: play_time,
                resolved_play_data: play_info,
                scheduled_for_stop,
            } => {
                if let Some(play_info) = play_info {
                    if scheduled_for_stop {
                        // Already scheduled for stop.
                        if args.stop_time == ClipStopTime::Immediately {
                            // Transition to stop now!
                            self.state = Suspending {
                                reason: SuspensionReason::Stop,
                                play_info,
                                transition_countdown: start_end_fade_length(),
                            };
                        }
                    } else {
                        // Not yet scheduled for stop.
                        let info = play_info.cursor_info_at(args.timeline_cursor_pos);
                        self.state = if info.has_started_already() {
                            // Playing
                            match args.stop_time {
                                ClipStopTime::Immediately => {
                                    // Immediately. Transition to stop.
                                    Suspending {
                                        reason: SuspensionReason::Stop,
                                        play_info,
                                        transition_countdown: start_end_fade_length(),
                                    }
                                }
                                ClipStopTime::EndOfClip => {
                                    // Schedule
                                    ClipState::ScheduledOrPlaying {
                                        play_instruction: play_time,
                                        resolved_play_data: Some(play_info),
                                        scheduled_for_stop: true,
                                    }
                                }
                            }
                        } else {
                            // Not yet playing. Backpedal.
                            ClipState::Stopped
                        };
                    }
                } else {
                    // Not yet playing. Backpedal.
                    self.state = ClipState::Stopped;
                }
            }
            Paused { .. } => {
                self.state = ClipState::Stopped;
            }
            Suspending {
                reason,
                play_info,
                transition_countdown,
            } => {
                if args.stop_time == ClipStopTime::Immediately && reason != SuspensionReason::Stop {
                    // We are in another transition already. Simply change it to stop.
                    self.state = Suspending {
                        reason: SuspensionReason::Stop,
                        play_info,
                        transition_countdown,
                    };
                }
            }
        }
    }

    fn seek_to(&mut self, args: SeekToArgs) {
        let length = self.native_clip_length();
        let desired_pos_in_secs =
            (length * args.desired_pos.get()).expect("proportional position never negative");
        use ClipState::*;
        match self.state {
            Stopped | Suspending { .. } => {}
            ScheduledOrPlaying {
                play_instruction: play_time,
                resolved_play_data: play_info,
                scheduled_for_stop,
            } => {
                if let Some(play_info) = play_info {
                    let info = play_info.cursor_info_at(args.timeline_cursor_pos);
                    if info.has_started_already() {
                        self.state = ClipState::ScheduledOrPlaying {
                            play_instruction: play_time,
                            resolved_play_data: Some(ResolvedPlayData {
                                next_block_pos: desired_pos_in_secs.into(),
                            }),
                            scheduled_for_stop,
                        };
                    }
                }
            }
            Paused { .. } => {
                self.state = Paused {
                    next_block_pos: desired_pos_in_secs,
                };
            }
        }
    }

    fn clip_length(&self, timeline_tempo: Bpm) -> DurationInSeconds {
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        DurationInSeconds::new(self.native_clip_length().get() / final_tempo_factor)
    }

    fn native_clip_length(&self) -> DurationInSeconds {
        if self.inner.is_midi {
            // For MIDI, get_length() takes the current project tempo in account ... which is not
            // what we want because we want to do all the tempo calculations ourselves and treat
            // MIDI/audio the same wherever possible.
            let beats = self
                .inner
                .source
                .get_length_beats()
                .expect("MIDI source must have length in beats");
            let beats_per_minute = self.inner.original_tempo();
            let beats_per_second = beats_per_minute.get() / 60.0;
            DurationInSeconds::new(beats.get() / beats_per_second)
        } else {
            self.inner.source.get_length().unwrap_or_default()
        }
    }

    fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.manual_tempo_factor = tempo_factor.max(MIN_TEMPO_FACTOR);
    }

    fn get_tempo_factor(&self) -> f64 {
        self.manual_tempo_factor
    }

    fn set_repeated(&mut self, args: SetRepeatedArgs) {
        self.repetition = {
            if args.repeated {
                Repetition::Infinitely
            } else {
                Repetition::Once
            }
        };
    }

    fn pos_within_clip(&self, args: PosWithinClipArgs) -> Option<PositionInSeconds> {
        use ClipState::*;
        let inner_source_pos = match self.state {
            ScheduledOrPlaying {
                resolved_play_data: Some(play_info),
                ..
            }
            | Suspending { play_info, .. } => play_info.next_block_pos,
            Paused { next_block_pos } => next_block_pos.into(),
            _ => return None,
        };
        let pos = inner_source_pos.get() / self.calc_final_tempo_factor(args.timeline_tempo);
        Some(PositionInSeconds::new(pos))
    }

    fn proportional_pos_within_clip(&self, args: PosWithinClipArgs) -> Option<UnitValue> {
        // TODO-medium This can be optimized
        let pos_within_clip = self.pos_within_clip(args);
        let length = self.clip_length(args.timeline_tempo);
        calculate_proportional_position(pos_within_clip, length)
    }
}

impl ClipPcmSourceSkills for BorrowedPcmSource {
    fn clip_state(&self) -> ClipState {
        let mut state = ClipState::Stopped;
        unsafe {
            self.extended(
                EXT_CLIP_STATE,
                &mut state as *mut _ as _,
                null_mut(),
                null_mut(),
            )
        };
        state
    }

    fn play(&mut self, mut args: PlayArgs) {
        unsafe {
            self.extended(EXT_PLAY, &mut args as *mut _ as _, null_mut(), null_mut());
        }
    }

    fn stop(&mut self, mut args: StopArgs) {
        unsafe {
            self.extended(EXT_STOP, &mut args as *mut _ as _, null_mut(), null_mut());
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

    fn seek_to(&mut self, mut args: SeekToArgs) {
        unsafe {
            self.extended(
                EXT_SEEK_TO,
                &mut args as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn clip_length(&self, mut timeline_tempo: Bpm) -> DurationInSeconds {
        let mut l = DurationInSeconds::MIN;
        unsafe {
            self.extended(
                EXT_CLIP_LENGTH,
                &mut timeline_tempo as *mut _ as _,
                &mut l as *mut _ as _,
                null_mut(),
            );
        }
        l
    }

    fn native_clip_length(&self) -> DurationInSeconds {
        let mut l = DurationInSeconds::MIN;
        unsafe {
            self.extended(
                EXT_NATIVE_CLIP_LENGTH,
                &mut l as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        l
    }

    fn set_tempo_factor(&mut self, mut tempo_factor: f64) {
        unsafe {
            self.extended(
                EXT_SET_TEMPO_FACTOR,
                &mut tempo_factor as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn set_repeated(&mut self, mut args: SetRepeatedArgs) {
        unsafe {
            self.extended(
                EXT_SET_REPEATED,
                &mut args as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }

    fn pos_within_clip(&self, mut args: PosWithinClipArgs) -> Option<PositionInSeconds> {
        let mut p: Option<PositionInSeconds> = None;
        unsafe {
            self.extended(
                EXT_POS_WITHIN_CLIP,
                &mut args as *mut _ as _,
                &mut p as *mut _ as _,
                null_mut(),
            );
        }
        p
    }

    fn proportional_pos_within_clip(&self, mut args: PosWithinClipArgs) -> Option<UnitValue> {
        let mut p: Option<UnitValue> = None;
        unsafe {
            self.extended(
                EXT_PROPORTIONAL_POS_WITHIN_CLIP,
                &mut args as *mut _ as _,
                &mut p as *mut _ as _,
                null_mut(),
            );
        }
        p
    }

    fn get_tempo_factor(&self) -> f64 {
        let mut f: f64 = 1.0;
        unsafe {
            self.extended(
                EXT_TEMPO_FACTOR,
                &mut f as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
        f
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
pub struct ResolvedPlayData {
    /// At the time `get_samples` is called, this contains the position in the inner source that
    /// should be played next.
    ///
    /// - It's a position *within* the inner source (modulo!).
    /// - If this position is negative, we are in the count-in phase. The count-in phase isn't
    ///   modulo.
    /// - On each call of `get_samples()`, the position is advanced and set *exactly* to the end of
    ///   the previous block, so that the source is played continuously under any circumstance,
    ///   without skipping material - because skipping material sounds bad.
    /// - Before introducing this field, we were instead memorizing the absolute timeline position
    ///   at which the clip started playing. Then we always played the source at the position that
    ///   corresponds to the current absolute timeline position - which is basically the analog to
    ///   putting items in the arrange view. It works flawlessly ... until you interact with the
    ///   timeline and/or make on-the-fly tempo changes. Read on!
    /// - First issue: The REAPER project timeline is
    ///   non-steady. It resets its position when we change the cursor position - even when the
    ///   project is not playing and therefore no sync is desired from ReaLearn's perspective.
    ///   The same happens when we change the tempo and the project is playing: The speed of the
    ///   timeline doesn't change (which is fine) but its position resets!
    /// - Second issue: While we could solve the first issue by consulting a steady timeline (e.g.
    ///   the preview register timeline), there's a second one that is about on-the-fly tempo
    ///   changes only. When increasing or decreasing the tempo, we really want the clip to play
    ///   continuously, with every sample block continuing at the position where it left off in the
    ///   previous block. That is the absolute basis for a smooth tempo changing experience. If we
    ///   calculate the position that should be played based on some distance-to-start logic using
    ///   a linear timeline, we will have a hard time achieving this. Because this logic assumes
    ///   that the tempo was always the same since the clip started playing.
    /// - For these reasons, we use this relative-to-previous-block logic. It guarantees that the
    ///   clip is played continuously, no matter what. Simple and effective.
    pub next_block_pos: PositionInSeconds,
}

impl ResolvedPlayData {
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
    play_info: ResolvedPlayData,
}

impl CursorInfo {
    fn has_started_already(&self) -> bool {
        self.play_info.next_block_pos >= PositionInSeconds::ZERO
    }
}

struct CursorAndLengthInfo {
    cursor_info: CursorInfo,
    /// This is the effective clip length, not the native one.
    clip_length: DurationInSeconds,
    repetition: Repetition,
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
const EXT_CLIP_STATE: i32 = 2359769;
const EXT_PLAY: i32 = 2359771;
const EXT_CLIP_LENGTH: i32 = 2359772;
const EXT_SET_REPEATED: i32 = 2359773;
const EXT_POS_WITHIN_CLIP: i32 = 2359775;
const EXT_STOP: i32 = 2359776;
const EXT_SEEK_TO: i32 = 2359778;
const EXT_PAUSE: i32 = 2359783;
const EXT_SET_TEMPO_FACTOR: i32 = 2359784;
const EXT_TEMPO_FACTOR: i32 = 2359785;
const EXT_NATIVE_CLIP_LENGTH: i32 = 2359786;
const EXT_PROPORTIONAL_POS_WITHIN_CLIP: i32 = 2359787;

struct FadeCalculator {
    /// End position, relative to start position zero.
    ///
    /// This is where the fade out will take place.
    pub end_pos: u64,
    /// Clip length.
    ///
    /// Used to calculate where's the repetition.
    pub clip_length: u64,
    /// Clip cursor offset.
    ///
    /// Used to calculate where's the repetition.
    pub clip_cursor_offset: u64,
    /// Length of the start fade-in and end fade-out.
    pub start_end_fade_length: u64,
    /// Length of the repetition fade-ins and fade-outs.
    pub intermediate_fade_length: u64,
}

impl FadeCalculator {
    pub fn calculate_fade_factor(&self, current_pos: i64) -> f64 {
        if current_pos < 0 {
            // Not yet playing
            return 0.0;
        }
        let current_pos = current_pos as u64;
        if current_pos >= self.end_pos {
            // Not playing anymore
            return 0.0;
        }
        // First, apply start-end fades (they have priority over intermediate fades).
        {
            let fade_length = self.start_end_fade_length;
            // Playing
            if current_pos < fade_length {
                // Playing the beginning: Fade in
                return current_pos as f64 / fade_length as f64;
            }
            let distance_to_end = self.end_pos - current_pos;
            if distance_to_end < fade_length {
                // Playing the end: Fade out
                return distance_to_end as f64 / fade_length as f64;
            }
        }
        // Intermediate repetition fades
        {
            let fade_length = self.intermediate_fade_length;
            let current_pos_within_clip = (current_pos as i64 + self.clip_cursor_offset as i64)
                .rem_euclid(self.clip_length as i64)
                as u64;
            let distance_to_clip_end = self.clip_length - current_pos_within_clip;
            if distance_to_clip_end < fade_length {
                // Approaching loop end: Fade out
                return distance_to_clip_end as f64 / fade_length as f64;
            }
            if current_pos_within_clip < fade_length {
                // Continuing at loop start: Fade in
                return current_pos_within_clip as f64 / fade_length as f64;
            }
        }
        // Normal playing
        1.0
    }
}

fn start_end_fade_length() -> DurationInSeconds {
    DurationInSeconds::new(0.01)
}

fn repetition_fade_length() -> DurationInSeconds {
    DurationInSeconds::new(0.01)
}

#[derive(Clone, Copy)]
pub struct StopArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
    pub stop_time: ClipStopTime,
}

#[derive(Clone, Copy)]
pub struct SeekToArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
    pub desired_pos: UnitValue,
}

#[derive(Clone, Copy)]
pub struct SetRepeatedArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
    pub repeated: bool,
}

#[derive(Clone, Copy)]
pub struct PosWithinClipArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
}

fn calculate_proportional_position(
    position: Option<PositionInSeconds>,
    length: DurationInSeconds,
) -> Option<UnitValue> {
    if length.get() == 0.0 {
        return Some(UnitValue::MIN);
    }
    position.map(|p| UnitValue::new_clamped(p.get() / length.get()))
}

const MIN_TEMPO_FACTOR: f64 = 0.0000000001;

struct BlockInfo {
    length: u32,
    sample_rate: Hz,
    duration: DurationInSeconds,
    cursor_and_lenght_info: CursorAndLengthInfo,
    final_tempo_factor: f64,
    stop_countdown: Option<DurationInSeconds>,
}

impl BlockInfo {
    pub fn new(
        block: &BorrowedPcmSourceTransfer,
        cursor_and_length_info: CursorAndLengthInfo,
        final_tempo_factor: f64,
        stop_countdown: Option<DurationInSeconds>,
    ) -> Self {
        let length = block.length() as u32;
        let sample_rate = block.sample_rate();
        let duration = DurationInSeconds::new(length as f64 / sample_rate.get());
        Self {
            length,
            sample_rate,
            duration,
            cursor_and_lenght_info: cursor_and_length_info,
            final_tempo_factor,
            stop_countdown,
        }
    }

    pub fn duration(&self) -> DurationInSeconds {
        self.duration
    }

    pub fn tempo_adjusted_duration(&self) -> DurationInSeconds {
        (self.duration * self.final_tempo_factor).expect("final tempo factor never negative")
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn sample_rate(&self) -> Hz {
        self.sample_rate
    }

    /// Negative position (count-in) or position *within* clip.
    pub fn block_start_pos(&self) -> PositionInSeconds {
        self.cursor_and_lenght_info
            .cursor_info
            .play_info
            .next_block_pos
    }

    pub fn block_end_pos(&self) -> PositionInSeconds {
        self.block_start_pos() + self.duration
    }

    pub fn tempo_adjusted_block_end_pos(&self) -> PositionInSeconds {
        self.block_start_pos() + self.tempo_adjusted_duration()
    }

    pub fn cursor_and_lenght_info(&self) -> &CursorAndLengthInfo {
        &self.cursor_and_lenght_info
    }

    pub fn stop_countdown(&self) -> Option<DurationInSeconds> {
        self.stop_countdown
    }

    pub fn final_tempo_factor(&self) -> f64 {
        self.final_tempo_factor
    }

    pub fn is_last_block(&self) -> bool {
        if let Some(cd) = self.stop_countdown {
            cd <= self.tempo_adjusted_duration()
        } else {
            false
        }
    }

    pub fn tempo_adjusted_sample_rate(&self) -> Hz {
        Hz::new(self.sample_rate.get() / self.final_tempo_factor)
    }
}
