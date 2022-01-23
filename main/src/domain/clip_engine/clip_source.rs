// TODO-medium Give this file a last overhaul as soon as things run as they should. There were many
//  changes and things might not be implemented/named optimally. Position naming is very
//  inconsistent at the moment.
use assert_no_alloc::*;
use crossbeam_channel::Sender;
use std::cmp;
use std::convert::TryInto;
use std::error::Error;
use std::ptr::null_mut;

use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::source_util::pcm_source_is_midi;
use crate::domain::clip_engine::supplier::stretcher::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::supplier::{
    AudioSupplier, ClipSupplierChain, ExactDuration, ExactFrameCount, LoopBehavior, Looper,
    MidiSupplier, StretchAudioMode, Stretcher, SupplyAudioRequest, SupplyMidiRequest,
    WithFrameRate, MIDI_BASE_BPM,
};
use crate::domain::clip_engine::{
    clip_timeline, clip_timeline_cursor_pos, ClipRecordMode, StretchWorkerRequest,
};
use crate::domain::Timeline;
use helgoboss_learn::UnitValue;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use reaper_high::{Project, Reaper};
use reaper_low::raw::{IReaperPitchShift, PCM_source_transfer_t, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::{
    BorrowedPcmSource, Bpm, CustomPcmSource, DurationInBeats, DurationInSeconds, ExtendedArgs,
    GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, MidiEvent, OwnedPcmSource, PcmSource,
    PcmSourceTransfer, PeaksClearArgs, PitchShiftMode, PitchShiftSubMode, PositionInSeconds,
    PropertiesWindowArgs, ReaperStr, SaveStateArgs, SetAvailableArgs, SetFileNameArgs,
    SetSourceArgs,
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
    /// Changes the tempo of this clip in addition to the natural tempo change.
    manual_tempo_factor: f64,
    /// An ever-increasing counter which is used just for debugging purposes at the moment.
    debug_counter: u64,
    /// The current state of this clip, containing only state which is non-derivable.
    state: ClipState,
    /// When a preview register plays this source, this field gets constantly updated with the
    /// sample rate used to play the source.
    current_sample_rate: Option<Hz>,
}

struct InnerSource {
    /// Caches the information if the inner clip source contains MIDI or audio material.
    kind: InnerSourceKind,
    chain: ClipSupplierChain,
}

#[derive(Copy, Clone)]
enum InnerSourceKind {
    Audio,
    Midi,
}

impl InnerSource {
    fn original_tempo(&self) -> Bpm {
        // TODO-high Correctly determine: For audio, guess depending on length or read metadata or
        //  let overwrite by user.
        // For MIDI, an arbitrary but constant value is enough!
        Bpm::new(MIDI_BASE_BPM)
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
    ScheduledOrPlaying(ScheduledOrPlayingState),
    /// Very short transition for fade outs or sending all-notes-off before entering another state.
    Suspending {
        reason: SuspensionReason,
        /// We still need the play info for fade out.
        play_info: ResolvedPlayData,
    },
    /// At this state, the clip is paused. No fade-in, no fade-out ... nothing.
    ///
    /// The player can stop in this state.
    Paused {
        /// Modulo position within the clip at which should be resumed later.
        next_block_pos: usize,
    },
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ScheduledOrPlayingState {
    pub play_instruction: PlayInstruction,
    /// Set as soon as the actual start has been resolved (from the play time field).
    pub resolved_play_data: Option<ResolvedPlayData>,
    pub scheduled_for_stop: bool,
    pub overdubbing: bool,
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
    PlayWhileSuspending { play_time: ClipStartTime },
}

#[derive(Clone, Copy)]
pub struct PlayArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub play_time: ClipStartTime,
    pub repetition: Repetition,
}

#[derive(Clone, Copy)]
pub struct RecordArgs {}

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct PlayInstruction {
    /// We consider the absolute scheduled play position as part of the instruction. It's important
    /// not to resolve it super-late because then each clip calculates its own positions, which
    /// can be bad when starting multiple clips at once, e.g. synced to REAPER transport.
    pub scheduled_play_pos: PositionInSeconds,
    pub start_pos_within_clip: DurationInSeconds,
    pub initial_tempo: Bpm,
}

impl PlayInstruction {
    fn from_play_time(
        play_time: ClipStartTime,
        timeline_cursor_pos: PositionInSeconds,
        timeline: impl Timeline,
    ) -> Self {
        use ClipStartTime::*;
        let scheduled_play_pos = match play_time {
            Immediately => timeline_cursor_pos,
            NextBar => timeline.next_bar_pos_at(timeline_cursor_pos),
        };
        Self {
            scheduled_play_pos,
            start_pos_within_clip: DurationInSeconds::ZERO,
            initial_tempo: timeline.tempo_at(timeline_cursor_pos),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ClipStartTime {
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
    pub fn new(
        inner: OwnedPcmSource,
        project: Option<Project>,
        stretch_worker_sender: &Sender<StretchWorkerRequest>,
    ) -> Self {
        Self {
            inner: InnerSource {
                kind: if pcm_source_is_midi(&inner) {
                    InnerSourceKind::Midi
                } else {
                    InnerSourceKind::Audio
                },
                chain: {
                    let mut chain = ClipSupplierChain::new(inner);
                    let looper = chain.looper_mut();
                    looper.set_fades_enabled(true);
                    let stretcher = chain.stretcher_mut();
                    stretcher.set_enabled(true);
                    chain
                },
            },
            project,
            debug_counter: 0,
            state: ClipState::Stopped,
            manual_tempo_factor: 1.0,
            current_sample_rate: None,
        }
    }

    fn calc_final_tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        let timeline_tempo_factor = timeline_tempo.get() / self.inner.original_tempo().get();
        // (self.manual_tempo_factor * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
        // TODO-medium Enable manual tempo factor at some point when everything is working.
        //  At the moment this introduces too many uncertainties and false positive bugs because
        //  our demo project makes it too easy to accidentally change the manual tempo.
        (1.0 * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
    }

    fn frame_within_clip(&self, timeline_tempo: Bpm) -> Option<isize> {
        use ClipState::*;
        let frame = match self.state {
            ScheduledOrPlaying(ScheduledOrPlayingState {
                resolved_play_data: Some(play_info),
                ..
            })
            | Suspending { play_info, .. } => {
                let pos = play_info.next_block_pos;
                if pos < 0 {
                    pos
                } else {
                    self.modulo_frame(pos as usize) as isize
                }
            }
            // Pause position is modulo already.
            Paused { next_block_pos } => next_block_pos as isize,
            _ => return None,
        };
        Some(frame)
    }

    fn schedule_play_internal(&mut self, args: PlayArgs) {
        self.inner
            .chain
            .looper_mut()
            .set_loop_behavior(LoopBehavior::from_repetition(args.repetition));
        self.state = ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
            play_instruction: PlayInstruction::from_play_time(
                args.play_time,
                args.timeline_cursor_pos,
                self.timeline(),
            ),
            ..Default::default()
        });
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
        let sample_rate = args.block.sample_rate();
        let timeline_cursor_frame = (timeline_cursor_pos.get() * sample_rate.get()) as isize;
        let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        // println!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
        //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
        self.current_sample_rate = Some(sample_rate);
        self.inner
            .chain
            .stretcher_mut()
            .set_tempo_factor(final_tempo_factor);
        use ClipState::*;
        match self.state {
            Stopped => {}
            Paused { .. } => {}
            Suspending { reason, play_info } => {
                let suspender = self.inner.chain.suspender_mut();
                if !suspender.is_suspending() {
                    suspender.suspend(play_info.next_block_pos);
                }
                self.state =
                    if let Some(end_frame) = self.fill_samples(args, play_info.next_block_pos) {
                        // Suspension not finished yet.
                        Suspending {
                            reason,
                            play_info: {
                                ResolvedPlayData {
                                    next_block_pos: end_frame,
                                }
                            },
                        }
                    } else {
                        // Suspension finished.
                        self.inner.chain.suspender_mut().reset();
                        self.get_suspension_follow_up_state(
                            reason,
                            play_info,
                            timeline_cursor_pos,
                            &timeline,
                        )
                    };
            }
            ScheduledOrPlaying(s) => {
                // Resolve play info if not yet resolved.
                let play_info = s.resolved_play_data.unwrap_or_else(|| {
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
                    let hypothetical_next_block_pos_in_secs =
                        timeline_cursor_pos - s.play_instruction.scheduled_play_pos;
                    let source_frame_rate = self.inner.chain.source().frame_rate();
                    let hypothetical_next_block_pos = (hypothetical_next_block_pos_in_secs.get()
                        * source_frame_rate.get())
                        as isize;
                    // let hypothetical_next_block_pos = timeline_cursor_frame - scheduled_play_frame;
                    let next_block_pos = if hypothetical_next_block_pos < 0 {
                        // Count-in phase.
                        println!(
                            "Count-in: hypothetical_next_block_pos = {}",
                            hypothetical_next_block_pos
                        );
                        let distance_to_start = -hypothetical_next_block_pos as usize;
                        // The scheduled play position was resolved taking the current project tempo
                        // into account! In order to keep advancing using our usual source-specific
                        // tempo factor later, we should fix the distance so it conforms to the tempo
                        // in which we advance the source play cursor.
                        // Example:
                        // - Native source tempo is 100 bpm
                        // - The tempo at schedule time was 120 bpm. The current distance_to_start
                        //   value was calculated assuming that this is the normal tempo.
                        // - However, from the perspective of the source, we had a final tempo
                        //   factor of 1.2 at that time.
                        // - We must correct distance_to_start so it is the distance from the
                        //   perspective of the source!
                        let next_block_pos =
                            -((distance_to_start as f64 * final_tempo_factor).round() as isize);
                        // TODO-medium Sometimes when raising tempo very much from initial low tempo
                        //  on count-in, the scheduled_play_pos gets insanely high compared to
                        //  timeline_cursor_pos. So the clip starts playing in 15secs or so...
                        if next_block_pos < -500000 {
                            dbg!(
                                hypothetical_next_block_pos_in_secs,
                                next_block_pos,
                                s.play_instruction.scheduled_play_pos,
                                sample_rate,
                                timeline_cursor_frame,
                                timeline_cursor_pos,
                                hypothetical_next_block_pos,
                                distance_to_start,
                                final_tempo_factor
                            );
                        }
                        next_block_pos
                    } else {
                        // Already playing.
                        // TODO-high Sometimes this happens when we turn the tempo very quickly
                        //  down during count-in. It destroys the timing completely. Reason:
                        //  The scheduled_play_pos is suddenly behind the timeline_cursor_pos.
                        //  I think the root cause (also with above opposite issue) is that
                        //  the timeline is not steady.
                        //  Solution 1: Use both for scheduling (main thread) and for resolving the
                        //  initial countdown value (audio thread) a steady timeline. For that, we
                        //  need to map the scheduled project timeline position (non-steady) to
                        //  a scheduled steady timeline position - at schedule time.
                        //  Solution 2: Don't schedule with absolute positions at all. Instead,
                        //  say "Next bar" and determine both scheduled position and initial
                        //  countdown value here. Problem: Batch scheduling of multiple clips could
                        //  lead to different results. Or wait: We *can* use absolute positions but
                        //  beat-based ones. E.g. Bar 510. The project timeline should be steady in
                        //  terms of beats at least. Let's do that.
                        println!(
                            "Already playing: hypothetical_next_block_pos = {}",
                            hypothetical_next_block_pos
                        );
                        dbg!(
                            hypothetical_next_block_pos_in_secs,
                            s.play_instruction.scheduled_play_pos,
                            sample_rate,
                            timeline_cursor_frame,
                            timeline_cursor_pos,
                            hypothetical_next_block_pos,
                            final_tempo_factor
                        );
                        hypothetical_next_block_pos
                    };

                    // if let InnerSourceKind::Audio { time_stretch_mode } = &mut self.inner.kind {
                    //     if let Some(TimeStretchMode::Serious(stretcher)) = time_stretch_mode {
                    //         stretcher.reset();
                    //     }
                    // }

                    // // This is the point where we advance the block position.
                    // let next_play_info = ResolvedPlayData {
                    //     next_block_pos: {
                    //         // TODO-medium This mechanism of advancing the position on every call by
                    //         //  the block duration relies on the fact that the preview
                    //         //  register timeline calls us continuously and never twice per block.
                    //         //  It would be better not to make that assumption and make this more
                    //         //  stable by actually looking at the diff between the currently requested
                    //         //  time_s and the previously requested time_s. If this diff is zero or
                    //         //  doesn't correspond to the non-tempo-adjusted block duration, we know
                    //         //  something is wrong.
                    //         if end_frame < 0 {
                    //             // This is still a *pure* count-in. No modulo logic yet.
                    //             // Also, we don't advance the position by a block duration that is
                    //             // adjusted using our normal tempo factor because at the time the
                    //             // initial countdown value was resolved, REAPER already took the current
                    //             // tempo into account. However, we must calculate a new tempo factor
                    //             //  based on possible tempo changes during the count-in phase!
                    //             // TODO-high When transport is not playing and we change the cursor
                    //             //  position, new count-ins in relation to the already playing clips
                    //             //  change. I think because the project timeline resets whenever we
                    //             //  change the cursor position, which makes the next-bar calculation
                    //             //  using a different origin. Crap.
                    //             // TODO-high Well, actually this happens also when the transport is
                    //             //  running, with the only difference that we also hear and see
                    //             //  the reset. Plus, when the transport is running, we want to
                    //             //  interrupt the clips and reschedule them. Still to be implemented.
                    //             let tempo_factor =
                    //                 timeline_tempo.get() / s.play_instruction.initial_tempo.get();
                    //             let duration =
                    //                 (block_info.frame_count() as f64 * tempo_factor) as usize;
                    //             block_info.start_frame() + duration as isize
                    //         } else {
                    //             // Playing already.
                    //             // Here we make sure that we always stay within the borders of the inner
                    //             // source. We don't use every-increasing positions because then tempo
                    //             // changes are not smooth anymore in subsequent cycles.
                    //             end_frame
                    //                 % self.native_clip_length_in_frames(block_info.sample_rate())
                    //                     as isize
                    //         }
                    //     },
                    // };
                    ResolvedPlayData { next_block_pos }
                });
                if s.scheduled_for_stop {
                    let looper = self.inner.chain.looper_mut();
                    let last_cycle = if play_info.next_block_pos < 0 {
                        0
                    } else {
                        looper.get_cycle_at_frame(play_info.next_block_pos as usize)
                    };
                    looper.set_loop_behavior(LoopBehavior::UntilEndOfCycle(last_cycle));
                }
                self.state =
                    if let Some(end_frame) = self.fill_samples(args, play_info.next_block_pos) {
                        // There's still something to play.
                        ScheduledOrPlaying(ScheduledOrPlayingState {
                            resolved_play_data: {
                                Some(ResolvedPlayData {
                                    next_block_pos: end_frame,
                                })
                            },
                            ..s
                        })
                    } else {
                        // We have reached the natural or scheduled end. Everything that needed to be
                        // played has been played in previous blocks. Audio fade outs have been applied
                        // as well, so no need to go to suspending state first. Go right to stop!
                        self.inner.chain.reset();
                        Stopped
                    };
            }
        }
    }

    fn get_suspension_follow_up_state(
        &mut self,
        reason: SuspensionReason,
        play_info: ResolvedPlayData,
        timeline_cursor_pos: PositionInSeconds,
        timeline: impl Timeline,
    ) -> ClipState {
        match reason {
            SuspensionReason::Retrigger => ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                play_instruction: PlayInstruction::from_play_time(
                    ClipStartTime::Immediately,
                    timeline_cursor_pos,
                    timeline,
                ),
                ..Default::default()
            }),
            SuspensionReason::Pause => {
                self.inner.chain.looper_mut().reset();
                ClipState::Paused {
                    next_block_pos: {
                        if play_info.next_block_pos < 0 {
                            0
                        } else {
                            self.modulo_frame(play_info.next_block_pos as usize)
                        }
                    },
                }
            }
            SuspensionReason::Stop => {
                self.inner.chain.reset();
                ClipState::Stopped
            }
            SuspensionReason::PlayWhileSuspending { play_time } => {
                ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                    play_instruction: PlayInstruction::from_play_time(
                        play_time,
                        timeline_cursor_pos,
                        timeline,
                    ),
                    ..Default::default()
                })
            }
        }
    }

    fn modulo_frame(&self, frame: usize) -> usize {
        frame % self.inner.chain.source().frame_count()
    }

    fn fill_samples(&mut self, args: &mut GetSamplesArgs, start_frame: isize) -> Option<isize> {
        // This means the clip is playing or about o play.
        // We want to start playing as soon as we reach the scheduled start position,
        // that means pos == 0.0. In order to do that, we need to take into account that
        // the audio buffer start point is not necessarily equal to the measure start
        // point. If we would naively start playing as soon as pos >= 0.0, we might skip
        // the first samples/messages! We need to start playing as soon as the end of
        // the audio block is located on or right to the scheduled start point
        // (end_pos >= 0.0).
        // if info.tempo_adjusted_end_frame() < 0 {
        //     // Complete block is located before start position (pure count-in block).
        //     return info.start_frame() + info.tempo_adjusted_frame_count() as isize;
        // }
        // At this point we are sure that the end of the block is right of the start position. The
        // start of the block might still be left of the start position (negative number).
        use InnerSourceKind::*;
        unsafe {
            match self.inner.kind {
                Audio => self.fill_samples_audio(args, start_frame),
                Midi => self.fill_samples_midi(args, start_frame),
            }
        }
    }
    unsafe fn fill_samples_audio(
        &self,
        args: &mut GetSamplesArgs,
        start_frame: isize,
    ) -> Option<isize> {
        let request = SupplyAudioRequest {
            start_frame,
            dest_sample_rate: args.block.sample_rate(),
        };
        let mut dest_buffer = AudioBufMut::from_raw(
            args.block.samples(),
            args.block.nch() as _,
            args.block.length() as _,
        );
        let response = self
            .inner
            .chain
            .head()
            .supply_audio(&request, &mut dest_buffer);
        response.next_inner_frame
    }

    fn fill_samples_midi(&self, args: &mut GetSamplesArgs, start_frame: isize) -> Option<isize> {
        let request = SupplyMidiRequest {
            start_frame,
            dest_frame_count: args.block.length() as _,
            dest_sample_rate: args.block.sample_rate(),
        };
        let response = self
            .inner
            .chain
            .head()
            .supply_midi(&request, args.block.midi_event_list());
        response.next_inner_frame
    }
}

impl CustomPcmSource for ClipPcmSource {
    fn duplicate(&mut self) -> Option<OwnedPcmSource> {
        // Not correct but probably never used.
        self.inner.chain.source().duplicate()
    }

    fn is_available(&mut self) -> bool {
        self.inner.chain.source().is_available()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        self.inner.chain.source().set_available(args.is_available);
    }

    fn get_type(&mut self) -> &ReaperStr {
        unsafe { self.inner.chain.source().get_type_unchecked() }
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unsafe { self.inner.chain.source().get_file_name_unchecked() }
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        self.inner.chain.source().set_file_name(args.new_file_name)
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        self.inner.chain.source().get_source()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        self.inner.chain.source().set_source(args.source);
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.inner.chain.source().get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        self.inner.chain.source().get_sample_rate()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        // The clip source itself can be considered to represent an infinite-length "track".
        DurationInSeconds::MAX
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        let _ = self.inner.chain.source().get_length_beats()?;
        Some(DurationInBeats::MAX)
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        self.inner.chain.source().get_bits_per_sample()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        self.inner.chain.source().get_preferred_position()
    }

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unsafe {
            self.inner
                .chain
                .source()
                .properties_window(args.parent_window)
        }
    }

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        assert_no_alloc(|| {
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
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unsafe {
            self.inner.chain.source().get_peak_info(args.block);
        }
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unsafe {
            self.inner.chain.source().save_state(args.context);
        }
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.inner
                .chain
                .source()
                .load_state(args.first_line, args.context)
        }
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
        self.inner.chain.source().peaks_clear(args.delete_file);
    }

    fn peaks_build_begin(&mut self) -> bool {
        self.inner.chain.source().peaks_build_begin()
    }

    fn peaks_build_run(&mut self) -> bool {
        self.inner.chain.source().peaks_build_run()
    }

    fn peaks_build_finish(&mut self) {
        self.inner.chain.source().peaks_build_finish();
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
            EXT_RECORD => {
                let inner_args = *(args.parm_1 as *mut _);
                self.record(inner_args);
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
            _ => {
                self.inner
                    .chain
                    .source()
                    .extended(args.call, args.parm_1, args.parm_2, args.parm_3)
            }
        }
    }
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

    /// Starts recording a clip.
    fn record(&mut self, args: RecordArgs);

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
            ScheduledOrPlaying(s) => {
                if s.scheduled_for_stop {
                    // Scheduled for stop. Backpedal!
                    self.state = ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                        scheduled_for_stop: false,
                        ..s
                    });
                } else {
                    // Scheduled for play or playing already.
                    if let Some(play_info) = s.resolved_play_data {
                        if play_info.has_started_already() {
                            // Already playing. Retrigger!
                            self.state = ClipState::Suspending {
                                reason: SuspensionReason::Retrigger,
                                play_info,
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
            Suspending { play_info, .. } => {
                // It's important to handle this, otherwise some play actions simply have no effect,
                // which is especially annoying when using transport sync because then it's like
                // forgetting that clip ... the next time the transport is stopped and started,
                // that clip won't play again.
                self.state = ClipState::Suspending {
                    reason: SuspensionReason::PlayWhileSuspending {
                        play_time: args.play_time,
                    },
                    play_info,
                };
            }
            // TODO-high We should do a fade-in!
            Paused { next_block_pos } => {
                // Resume
                self.state = ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                    play_instruction: Default::default(),
                    resolved_play_data: Some(ResolvedPlayData {
                        next_block_pos: next_block_pos as isize,
                    }),
                    scheduled_for_stop: false,
                    overdubbing: false,
                });
            }
        }
    }

    fn pause(&mut self, timeline_cursor_pos: PositionInSeconds) {
        use ClipState::*;
        match self.state {
            Stopped | Paused { .. } => {}
            ScheduledOrPlaying(ScheduledOrPlayingState {
                resolved_play_data: play_info,
                ..
            }) => {
                if let Some(play_info) = play_info {
                    if play_info.has_started_already() {
                        // Playing. Pause!
                        // (If this clip is scheduled for stop already, a pause will backpedal from
                        // that.)
                        self.state = ClipState::Suspending {
                            reason: SuspensionReason::Pause,
                            play_info,
                        };
                    }
                }
                // If not yet playing, we don't do anything at the moment.
                // TODO-medium In future, we could defer the clip scheduling to the future. I think
                //  that would feel natural.
            }
            Suspending { reason, play_info } => {
                if reason != SuspensionReason::Pause {
                    // We are in another transition already. Simply change it to pause.
                    self.state = ClipState::Suspending {
                        reason: SuspensionReason::Pause,
                        play_info,
                    };
                }
            }
        }
    }

    fn record(&mut self, args: RecordArgs) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            ScheduledOrPlaying(s) => {
                self.state = ScheduledOrPlaying(ScheduledOrPlayingState {
                    overdubbing: true,
                    ..s
                });
            }
            Suspending { .. } => {}
            Paused { .. } => {}
        }
    }

    fn stop(&mut self, args: StopArgs) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            ScheduledOrPlaying(s) => {
                if let Some(play_info) = s.resolved_play_data {
                    if s.scheduled_for_stop {
                        // Already scheduled for stop.
                        if args.stop_time == ClipStopTime::Immediately {
                            // Transition to stop now!
                            self.state = Suspending {
                                reason: SuspensionReason::Stop,
                                play_info,
                            };
                        }
                    } else {
                        // Not yet scheduled for stop.
                        self.state = if play_info.has_started_already() {
                            // Playing
                            match args.stop_time {
                                ClipStopTime::Immediately => {
                                    // Immediately. Transition to stop.
                                    Suspending {
                                        reason: SuspensionReason::Stop,
                                        play_info,
                                    }
                                }
                                ClipStopTime::EndOfClip => {
                                    // Schedule
                                    ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                                        scheduled_for_stop: true,
                                        ..s
                                    })
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
            Suspending { reason, play_info } => {
                if args.stop_time == ClipStopTime::Immediately && reason != SuspensionReason::Stop {
                    // We are in another transition already. Simply change it to stop.
                    self.state = Suspending {
                        reason: SuspensionReason::Stop,
                        play_info,
                    };
                }
            }
        }
    }

    fn seek_to(&mut self, args: SeekToArgs) {
        let frame_count = self.inner.chain.source().frame_count();
        let desired_frame = (frame_count as f64 * args.desired_pos.get()).round() as usize;
        use ClipState::*;
        match self.state {
            Stopped | Suspending { .. } => {}
            ScheduledOrPlaying(s) => {
                if let Some(play_info) = s.resolved_play_data {
                    if play_info.has_started_already() {
                        self.state = ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                            resolved_play_data: Some(ResolvedPlayData {
                                next_block_pos: desired_frame as isize,
                            }),
                            ..s
                        });
                    }
                }
            }
            Paused { .. } => {
                self.state = Paused {
                    next_block_pos: desired_frame,
                };
            }
        }
    }

    fn clip_length(&self, timeline_tempo: Bpm) -> DurationInSeconds {
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        DurationInSeconds::new(self.native_clip_length().get() / final_tempo_factor)
    }

    fn native_clip_length(&self) -> DurationInSeconds {
        self.inner.chain.source().duration()
    }

    fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.manual_tempo_factor = tempo_factor.max(MIN_TEMPO_FACTOR);
    }

    fn get_tempo_factor(&self) -> f64 {
        self.manual_tempo_factor
    }

    fn set_repeated(&mut self, args: SetRepeatedArgs) {
        self.inner
            .chain
            .looper_mut()
            .set_loop_behavior(LoopBehavior::from_bool(args.repeated))
    }

    fn pos_within_clip(&self, args: PosWithinClipArgs) -> Option<PositionInSeconds> {
        let sr = self.current_sample_rate?;
        let frame = self.frame_within_clip(args.timeline_tempo)?;
        let second = frame as f64 / sr.get();
        Some(PositionInSeconds::new(second))
    }

    fn proportional_pos_within_clip(&self, args: PosWithinClipArgs) -> Option<UnitValue> {
        let frame_within_clip = self.frame_within_clip(args.timeline_tempo)?;
        if frame_within_clip < 0 {
            None
        } else {
            let frame_count = self.inner.chain.source().frame_count();
            if frame_count == 0 {
                Some(UnitValue::MIN)
            } else {
                let proportional =
                    UnitValue::new_clamped(frame_within_clip as f64 / frame_count as f64);
                Some(proportional)
            }
        }
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

    fn record(&mut self, mut args: RecordArgs) {
        unsafe {
            self.extended(EXT_RECORD, &mut args as *mut _ as _, null_mut(), null_mut());
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

#[derive(Copy, Clone, Debug)]
pub struct ResolvedPlayData {
    /// At the time `get_samples` is called, this contains the position in the inner source that
    /// should be played next.
    ///
    /// - The frames relate to the source sample rate.
    /// - The position can be after the source content, in which case one needs to modulo native
    ///   source length to get the position *within* the inner source.
    /// - If this position is negative, we are in the count-in phase.
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
    pub next_block_pos: isize,
}

impl ResolvedPlayData {
    fn has_started_already(&self) -> bool {
        self.next_block_pos >= 0
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
const EXT_RECORD: i32 = 2359788;

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

const MIN_TEMPO_FACTOR: f64 = 0.0000000001;
