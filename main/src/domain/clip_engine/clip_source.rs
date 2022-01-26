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
    adjust_proportionally, adjust_proportionally_positive, clip_timeline, clip_timeline_cursor_pos,
    convert_duration_in_frames_to_other_frame_rate, convert_duration_in_frames_to_seconds,
    convert_duration_in_seconds_to_frames, convert_position_in_frames_to_seconds,
    convert_position_in_seconds_to_frames, ClipRecordMode, StretchWorkerRequest,
    SupplyRequestGeneralInfo, SupplyRequestInfo, WithTempo,
};
use crate::domain::Timeline;
use helgoboss_learn::UnitValue;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use reaper_high::{Project, Reaper};
use reaper_low::raw::{
    midi_realtime_write_struct_t, IReaperPitchShift, PCM_source_transfer_t,
    REAPER_PITCHSHIFT_API_VER,
};
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, Bpm, CustomPcmSource, DurationInBeats,
    DurationInSeconds, ExtendedArgs, GetPeakInfoArgs, GetSamplesArgs, Hz, LoadStateArgs, MidiEvent,
    OwnedPcmSource, PcmSource, PcmSourceTransfer, PeaksClearArgs, PitchShiftMode,
    PitchShiftSubMode, PositionInSeconds, PropertiesWindowArgs, ReaperStr, SaveStateArgs,
    SetAvailableArgs, SetFileNameArgs, SetSourceArgs,
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
    previous_bar: i32,
}

struct InnerSource {
    /// Caches the information if the inner clip source contains MIDI or audio material.
    kind: InnerSourceKind,
    chain: ClipSupplierChain,
    tempo: Bpm,
    beat_count: u32,
}

#[derive(Copy, Clone)]
enum InnerSourceKind {
    Audio,
    Midi,
}

impl InnerSource {
    fn tempo(&self) -> Bpm {
        self.tempo
    }

    fn beat_count(&self) -> u32 {
        self.beat_count
    }

    fn bar_count(&self) -> u32 {
        // TODO-high Respect different time signatures
        self.beat_count / 4
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
        /// Modulo position within the inner source at which should be resumed later.
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
    Retrigger { scheduled_for_bar: Option<i32> },
    /// Play was suspended for initiating a pause, so the next state will be [`ClipState::Paused`].
    Pause,
    /// Play was suspended for initiating a stop, so the next state will be [`ClipState::Stopped`].
    Stop,
}

#[derive(Clone, Copy)]
pub struct PlayArgs {
    pub timeline_cursor_pos: PositionInSeconds,
    pub scheduled_for_bar: Option<i32>,
    pub repetition: Repetition,
}

#[derive(Clone, Copy)]
pub struct RecordArgs {}

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct PlayInstruction {
    pub scheduled_for_bar: Option<i32>,
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
        let is_midi = pcm_source_is_midi(&inner);
        let tempo = inner
            .tempo()
            .unwrap_or_else(|| detect_tempo(inner.duration(), project));
        let beat_count = if is_midi {
            inner
                .get_length_beats()
                .expect("MIDI source should report beats")
                .get()
                .round() as u32
        } else {
            let beats_per_sec = tempo.get() / 60.0;
            (inner.duration().get() * beats_per_sec).round() as u32
        };
        Self {
            inner: InnerSource {
                tempo,
                kind: if is_midi {
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
                    let serious = SeriousTimeStretcher::new();
                    // stretcher.set_mode(StretchAudioMode::Serious(serious));
                    chain
                },
                beat_count,
            },
            project,
            debug_counter: 0,
            state: ClipState::Stopped,
            manual_tempo_factor: 1.0,
            current_sample_rate: None,
            previous_bar: 0,
        }
    }

    fn calc_final_tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        let timeline_tempo_factor = timeline_tempo.get() / self.inner.tempo().get();
        if let Some(f) = FIXED_TEMPO_FACTOR {
            f
        } else {
            // TODO-medium Enable manual tempo factor at some point when everything is working.
            //  At the moment this introduces too many uncertainties and false positive bugs because
            //  our demo project makes it too easy to accidentally change the manual tempo.
            (1.0 * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
            // (self.manual_tempo_factor * timeline_tempo_factor).max(MIN_TEMPO_FACTOR)
        }
    }

    fn frame_within_inner_source(&self) -> Option<isize> {
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
            play_instruction: PlayInstruction {
                scheduled_for_bar: args.scheduled_for_bar,
            },
            ..Default::default()
        });
    }

    fn get_samples_internal(&mut self, args: &mut GetSamplesArgs, timeline: impl Timeline) {
        let timeline_cursor_pos = timeline.cursor_pos();
        let timeline_tempo = timeline.tempo_at(timeline_cursor_pos);
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        // println!("block sr = {}, block length = {}, block time = {}, timeline cursor pos = {}, timeline cursor frame = {}",
        //          sample_rate, args.block.length(), args.block.time_s(), timeline_cursor_pos, timeline_cursor_frame);
        let general_info = SupplyRequestGeneralInfo {
            audio_block_timeline_cursor_pos: timeline_cursor_pos,
            audio_block_length: args.block.length() as usize,
            output_frame_rate: args.block.sample_rate(),
            timeline_tempo,
            clip_tempo_factor: final_tempo_factor,
        };
        self.current_sample_rate = Some(args.block.sample_rate());
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
                self.state = if let Some(end_frame) =
                    self.fill_samples(args, play_info.next_block_pos, &general_info)
                {
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
                    self.get_suspension_follow_up_state(reason, play_info)
                };
            }
            ScheduledOrPlaying(s) => {
                // Resolve play info if not yet resolved.
                let play_info = s.resolved_play_data.unwrap_or_else(|| {
                    // So, this is how we do play scheduling. Whenever the preview register
                    // calls get_samples() and we are in a fresh ScheduledOrPlaying state, the
                    // relative number of count-in frames will be determined. Based on the given
                    // absolute bar for which the clip is scheduled.
                    // 1. We use a *relative* count-in (instead of just
                    // using the absolute scheduled-play position and check if we reached it)
                    // in order to respect arbitrary tempo changes during the count-in phase and
                    // still end up starting on the correct point in time. Okay, we could reach
                    // the same goal also by regularly checking whether we finally reached the
                    // start of the bar. But first, we need the relative count-in later anyway
                    // (for pickup beats, which start to play during count-in time). And second,
                    // it would be also pretty much unnecessary beat-time mapping.
                    // 2. We resolve the
                    // count-in length here in the real-time context, not before! In particular not
                    // at the time the play is requested. At that time we just calculate the
                    // bar index. Reason: The start time of the next bar at play-request time
                    // is not necessarily the same as the one in the get_samples call. If it's not,
                    // we would start advancing the count-in cursor from a wrong initial state
                    // and therefore end up with the wrong point in time for starting the clip
                    // (too late, to be accurate, because we would start advancing too late).
                    // TODO-high Well, actually this happens also when the transport is
                    //  running, with the only difference that we also hear and see
                    //  the reset. Plus, when the transport is running, we want to
                    //  interrupt the clips and reschedule them. Still to be implemented.
                    let next_block_pos = if let Some(start_bar) =
                        s.play_instruction.scheduled_for_bar
                    {
                        // Basics
                        let block_length_in_timeline_frames = args.block.length() as usize;
                        let source_frame_rate = self.inner.chain.reaper_source().frame_rate();
                        let timeline_frame_rate = args.block.sample_rate();
                        // Essential calculation
                        let start_bar_timeline_pos = timeline.pos_of_bar(start_bar);
                        let rel_pos_from_bar_in_secs =
                            timeline_cursor_pos - start_bar_timeline_pos;
                        // Natural deviation logging
                        {
                            // Assuming a constant tempo and time signature during one cycle
                            // Bars
                            let end_bar = start_bar + self.inner.bar_count() as i32;
                            let bar_count = end_bar - start_bar;
                            let end_bar_timeline_pos = timeline.pos_of_bar(end_bar);
                            assert!(end_bar_timeline_pos > start_bar_timeline_pos);
                            // Timeline cycle length
                            let timeline_cycle_length_in_secs =
                                (end_bar_timeline_pos - start_bar_timeline_pos).abs();
                            let timeline_cycle_length_in_timeline_frames =
                                convert_duration_in_seconds_to_frames(
                                    timeline_cycle_length_in_secs,
                                    timeline_frame_rate,
                                );
                            let timeline_cycle_length_in_source_frames =
                                convert_duration_in_seconds_to_frames(
                                    timeline_cycle_length_in_secs,
                                    source_frame_rate,
                                );
                            // Source cycle length
                            let source_cycle_length_in_secs = self.inner.chain.reaper_source().duration();
                            let source_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
                                source_cycle_length_in_secs,
                                timeline_frame_rate
                            );
                            let source_cycle_length_in_source_frames = self.inner.chain.reaper_source().frame_count();
                            // Block length
                            let block_length_in_timeline_frames = args.block.length() as usize;
                            let block_length_in_secs = convert_duration_in_frames_to_seconds(
                                block_length_in_timeline_frames, timeline_frame_rate
                            );
                            let block_length_in_source_frames =
                                convert_duration_in_frames_to_other_frame_rate(
                                    block_length_in_timeline_frames,
                                    timeline_frame_rate,
                                    source_frame_rate,
                                );
                            // Block count and remainder
                            let num_full_blocks = source_cycle_length_in_source_frames / block_length_in_source_frames;
                            let remainder_in_source_frames = source_cycle_length_in_source_frames % block_length_in_source_frames;
                            // Tempo-adjusted
                            let adjusted_block_length_in_source_frames = adjust_proportionally_positive(block_length_in_source_frames as f64, final_tempo_factor);
                            let adjusted_block_length_in_timeline_frames = convert_duration_in_frames_to_other_frame_rate(
                                adjusted_block_length_in_source_frames, source_frame_rate, timeline_frame_rate
                            );
                            let adjusted_block_length_in_secs = convert_duration_in_frames_to_seconds(
                                adjusted_block_length_in_source_frames,
                                source_frame_rate
                            );
                            let adjusted_remainder_in_source_frames = adjust_proportionally_positive(remainder_in_source_frames as f64, final_tempo_factor);
                            // Source cycle remainder
                            let adjusted_remainder_in_timeline_frames =
                                convert_duration_in_frames_to_other_frame_rate(
                                    adjusted_remainder_in_source_frames,
                                    source_frame_rate,
                                    timeline_frame_rate,
                                );
                            let adjusted_remainder_in_secs =
                                convert_duration_in_frames_to_seconds(
                                    adjusted_remainder_in_source_frames,
                                    source_frame_rate,
                                );
                            let block_index = (timeline_cursor_pos.get() / block_length_in_secs.get()) as isize;
                            print!(
                                "\n\
                                # Natural deviation report\n\
                                Block index: {},\n\
                                Tempo factor: {:.3}\n\
                                Bars: {} ({} - {})\n\
                                Start bar position: {:.3}s\n\
                                Source cycle length: {:.3}ms (= {} timeline frames = {} source frames)\n\
                                Timeline cycle length: {:.3}ms (= {} timeline frames = {} source frames)\n\
                                Block length: {:.3}ms (= {} timeline frames = {} source frames)\n\
                                Tempo-adjusted block length: {:.3}ms (= {} timeline frames = {} source frames)\n\
                                Number of full blocks: {}\n\
                                Tempo-adjusted remainder per cycle: {:.3}ms (= {} timeline frames = {} source frames)\n\
                                ",
                                block_index,

                                final_tempo_factor,

                                bar_count, start_bar, end_bar,

                                start_bar_timeline_pos.get(),

                                source_cycle_length_in_secs.get() * 1000.0,
                                source_cycle_length_in_timeline_frames,
                                source_cycle_length_in_source_frames,

                                timeline_cycle_length_in_secs.get() * 1000.0,
                                timeline_cycle_length_in_timeline_frames,
                                timeline_cycle_length_in_source_frames,

                                block_length_in_secs.get() * 1000.0,
                                block_length_in_timeline_frames,
                                block_length_in_source_frames,

                                adjusted_block_length_in_secs.get() * 1000.0,
                                adjusted_block_length_in_timeline_frames,
                                adjusted_block_length_in_source_frames,

                                num_full_blocks,

                                adjusted_remainder_in_secs.get() * 1000.0,
                                adjusted_remainder_in_timeline_frames,
                                adjusted_remainder_in_source_frames,
                            );
                        }
                        let rel_pos_from_bar_in_source_frames = convert_position_in_seconds_to_frames(
                            rel_pos_from_bar_in_secs,
                            self.inner.chain.reaper_source().frame_rate(),
                        );
                        // Now we have a countdown/position in source frames, but it doesn't yet
                        // take the tempo adjustment of the source into account. 
                        // Once we have initialized the countdown with the first value, each 
                        // get_samples() call - including this one - will advance it by a frame 
                        // count that ideally = block length in source frames * tempo factor.
                        // We use this countdown approach for two reasons.
                        //
                        // 1. In order to allow tempo changes during count-in time.
                        // 2. In future, the count-in phase might play source material already.
                        //
                        // Especially (2) means that the count-in phase will not always have that
                        // ideal length which makes the source frame ZERO be perfectly aligned with
                        // the ZERO of the timeline bar. I think this is unavoidable when dealing
                        // with material that needs sample-rate conversion and/or time
                        // stretching. So if one of this is involved, this is just an estimation.
                        // However, in real-world scenarios this usually results in slight start
                        // deviations around 0-5ms, so it still makes sense musically.

                        /// It can make a difference if we apply a factor once on a large integer x and then round or
                        /// n times on x/n and round each time. Latter is what happens in practice because we advance frames step by step in n blocks.
                        fn adjust_proportionally_in_blocks(value: isize, factor: f64, block_length: usize) -> isize {
                            let abs_value = value.abs() as usize;
                            let block_count = abs_value / block_length;
                            let remainder = abs_value % block_length;
                            let adjusted_block_length = adjust_proportionally_positive(block_length as f64, factor);
                            let adjusted_remainder = adjust_proportionally_positive(remainder as f64, factor);
                            let total_without_remainder = block_count * adjusted_block_length;
                            let total = total_without_remainder + adjusted_remainder;
                            dbg!(abs_value, adjusted_block_length, block_count, remainder, adjusted_remainder, total_without_remainder, total);
                            total as isize * value.signum()
                        }
                        let block_length_in_source_frames =
                            convert_duration_in_frames_to_other_frame_rate(
                                block_length_in_timeline_frames,
                                timeline_frame_rate,
                                source_frame_rate,
                            );
                        adjust_proportionally_in_blocks(rel_pos_from_bar_in_source_frames, final_tempo_factor, block_length_in_source_frames)
                    } else {
                        0
                    };
                    ResolvedPlayData { next_block_pos }
                });
                self.state = if let Some(end_frame) =
                    self.fill_samples(args, play_info.next_block_pos, &general_info)
                {
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
    ) -> ClipState {
        match reason {
            SuspensionReason::Retrigger { scheduled_for_bar } => {
                ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
                    play_instruction: PlayInstruction { scheduled_for_bar },
                    ..Default::default()
                })
            }
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
        }
    }

    fn modulo_frame(&self, frame: usize) -> usize {
        frame % self.inner.chain.reaper_source().frame_count()
    }

    fn fill_samples(
        &mut self,
        args: &mut GetSamplesArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
    ) -> Option<isize> {
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
                Audio => self.fill_samples_audio(args, start_frame, info),
                Midi => self.fill_samples_midi(args, start_frame, info),
            }
        }
    }

    unsafe fn fill_samples_audio(
        &self,
        args: &mut GetSamplesArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
    ) -> Option<isize> {
        let request = SupplyAudioRequest {
            start_frame,
            dest_sample_rate: args.block.sample_rate(),
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-audio",
                note: "",
            },
            parent_request: None,
            general_info: info,
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

    fn fill_samples_midi(
        &self,
        args: &mut GetSamplesArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
    ) -> Option<isize> {
        let request = SupplyMidiRequest {
            start_frame,
            dest_frame_count: args.block.length() as _,
            dest_sample_rate: args.block.sample_rate(),
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-midi",
                note: "",
            },
            parent_request: None,
            general_info: info,
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
        self.inner.chain.reaper_source().duplicate()
    }

    fn is_available(&mut self) -> bool {
        self.inner.chain.reaper_source().is_available()
    }

    fn set_available(&mut self, args: SetAvailableArgs) {
        self.inner
            .chain
            .reaper_source()
            .set_available(args.is_available);
    }

    fn get_type(&mut self) -> &ReaperStr {
        unsafe { self.inner.chain.reaper_source().get_type_unchecked() }
    }

    fn get_file_name(&mut self) -> Option<&ReaperStr> {
        unsafe { self.inner.chain.reaper_source().get_file_name_unchecked() }
    }

    fn set_file_name(&mut self, args: SetFileNameArgs) -> bool {
        self.inner
            .chain
            .reaper_source()
            .set_file_name(args.new_file_name)
    }

    fn get_source(&mut self) -> Option<PcmSource> {
        self.inner.chain.reaper_source().get_source()
    }

    fn set_source(&mut self, args: SetSourceArgs) {
        self.inner.chain.reaper_source().set_source(args.source);
    }

    fn get_num_channels(&mut self) -> Option<u32> {
        self.inner.chain.reaper_source().get_num_channels()
    }

    fn get_sample_rate(&mut self) -> Option<Hz> {
        self.inner.chain.reaper_source().get_sample_rate()
    }

    fn get_length(&mut self) -> DurationInSeconds {
        // The clip source itself can be considered to represent an infinite-length "track".
        DurationInSeconds::MAX
    }

    fn get_length_beats(&mut self) -> Option<DurationInBeats> {
        let _ = self.inner.chain.reaper_source().get_length_beats()?;
        Some(DurationInBeats::MAX)
    }

    fn get_bits_per_sample(&mut self) -> u32 {
        self.inner.chain.reaper_source().get_bits_per_sample()
    }

    fn get_preferred_position(&mut self) -> Option<PositionInSeconds> {
        self.inner.chain.reaper_source().get_preferred_position()
    }

    fn properties_window(&mut self, args: PropertiesWindowArgs) -> i32 {
        unsafe {
            self.inner
                .chain
                .reaper_source()
                .properties_window(args.parent_window)
        }
    }

    fn get_samples(&mut self, mut args: GetSamplesArgs) {
        assert_no_alloc(|| {
            // Make sure that in any case, we are only queried once per time, without retries.
            // TODO-medium This mechanism of advancing the position on every call by
            //  the block duration relies on the fact that the preview
            //  register timeline calls us continuously and never twice per block.
            //  It would be better not to make that assumption and make this more
            //  stable by actually looking at the diff between the currently requested
            //  time_s and the previously requested time_s. If this diff is zero or
            //  doesn't correspond to the non-tempo-adjusted block duration, we know
            //  something is wrong.
            unsafe {
                args.block.set_samples_out(args.block.length());
            }
            // Get main timeline info
            let timeline = clip_timeline(self.project, false);
            if !timeline.is_running() {
                // Main timeline is paused. Don't play, we don't want to play the same buffer
                // repeatedly!
                // TODO-high Pausing main transport and continuing has timing issues.
                return;
            }
            // Get samples
            self.get_samples_internal(&mut args, timeline);
        });
        debug_assert_eq!(args.block.samples_out(), args.block.length());
    }

    fn get_peak_info(&mut self, args: GetPeakInfoArgs) {
        unsafe {
            self.inner.chain.reaper_source().get_peak_info(args.block);
        }
    }

    fn save_state(&mut self, args: SaveStateArgs) {
        unsafe {
            self.inner.chain.reaper_source().save_state(args.context);
        }
    }

    fn load_state(&mut self, args: LoadStateArgs) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.inner
                .chain
                .reaper_source()
                .load_state(args.first_line, args.context)
        }
    }

    fn peaks_clear(&mut self, args: PeaksClearArgs) {
        self.inner
            .chain
            .reaper_source()
            .peaks_clear(args.delete_file);
    }

    fn peaks_build_begin(&mut self) -> bool {
        self.inner.chain.reaper_source().peaks_build_begin()
    }

    fn peaks_build_run(&mut self) -> bool {
        self.inner.chain.reaper_source().peaks_build_run()
    }

    fn peaks_build_finish(&mut self) {
        self.inner.chain.reaper_source().peaks_build_finish();
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
            EXT_WRITE_MIDI => {
                let inner_args = *(args.parm_1 as *mut _);
                self.write_midi(inner_args);
                1
            }
            EXT_SET_REPEATED => {
                let inner_args = *(args.parm_1 as *mut _);
                self.set_repeated(inner_args);
                1
            }
            _ => self.inner.chain.reaper_source().extended(
                args.call,
                args.parm_1,
                args.parm_2,
                args.parm_3,
            ),
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

    fn write_midi(&mut self, request: WriteMidiRequest);
}

#[derive(Copy, Clone)]
pub struct WriteMidiRequest<'a> {
    pub pos_within_clip: PositionInSeconds,
    pub input_sample_rate: Hz,
    pub block_length: usize,
    pub events: &'a BorrowedMidiEventList,
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
                    self.inner
                        .chain
                        .looper_mut()
                        .set_loop_behavior(LoopBehavior::Infinitely);
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
                                reason: SuspensionReason::Retrigger {
                                    scheduled_for_bar: args.scheduled_for_bar,
                                },
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
                    reason: SuspensionReason::Retrigger {
                        scheduled_for_bar: args.scheduled_for_bar,
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
                                    self.inner
                                        .chain
                                        .looper_mut()
                                        .keep_playing_until_end_of_current_cycle(
                                            play_info.next_block_pos,
                                        );
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
        let frame_count = self.inner.chain.reaper_source().frame_count();
        let desired_frame =
            adjust_proportionally_positive(frame_count as f64, args.desired_pos.get());
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
        self.inner.chain.reaper_source().duration()
    }

    fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.manual_tempo_factor = tempo_factor.max(MIN_TEMPO_FACTOR);
    }

    fn get_tempo_factor(&self) -> f64 {
        self.manual_tempo_factor
    }

    fn set_repeated(&mut self, args: SetRepeatedArgs) {
        let looper = self.inner.chain.looper_mut();
        if args.repeated {
            looper.set_loop_behavior(LoopBehavior::Infinitely);
        } else if let ClipState::ScheduledOrPlaying(ScheduledOrPlayingState {
            resolved_play_data: Some(d),
            ..
        }) = self.state
        {
            looper.keep_playing_until_end_of_current_cycle(d.next_block_pos);
        } else {
            looper.set_loop_behavior(LoopBehavior::UntilEndOfCycle(0));
        }
    }

    fn pos_within_clip(&self, args: PosWithinClipArgs) -> Option<PositionInSeconds> {
        let source_pos_in_source_frames = self.frame_within_inner_source()?;
        let source_pos_in_secs = convert_position_in_frames_to_seconds(
            source_pos_in_source_frames,
            self.inner.chain.reaper_source().frame_rate(),
        );
        let final_tempo_factor = self.calc_final_tempo_factor(args.timeline_tempo);
        let source_pos_in_secs_tempo_adjusted = source_pos_in_secs.get() / final_tempo_factor;
        Some(PositionInSeconds::new(source_pos_in_secs_tempo_adjusted))
    }

    fn proportional_pos_within_clip(&self, args: PosWithinClipArgs) -> Option<UnitValue> {
        let frame_within_clip = self.frame_within_inner_source()?;
        if frame_within_clip < 0 {
            None
        } else {
            let frame_count = self.inner.chain.reaper_source().frame_count();
            if frame_count == 0 {
                Some(UnitValue::MIN)
            } else {
                let proportional =
                    UnitValue::new_clamped(frame_within_clip as f64 / frame_count as f64);
                Some(proportional)
            }
        }
    }

    fn write_midi(&mut self, request: WriteMidiRequest) {
        let mut write_struct = midi_realtime_write_struct_t {
            // TODO-medium The following values work for arbitrary REAPER tempos, but
            //  not sure if they work for custom tempo factors.
            global_time: request.pos_within_clip.get(),
            srate: request.input_sample_rate.get(),
            item_playrate: 1.0,
            global_item_time: 0.0,
            length: request.block_length as _,
            // Overdub
            overwritemode: 0,
            events: unsafe { request.events.as_ptr().as_mut() },
            latency: 0.0,
            // Not used
            overwrite_actives: null_mut(),
        };
        const PCM_SOURCE_EXT_ADDMIDIEVENTS: i32 = 0x10005;
        unsafe {
            self.inner.chain.reaper_source_mut().extended(
                PCM_SOURCE_EXT_ADDMIDIEVENTS,
                &mut write_struct as *mut _ as _,
                null_mut(),
                null_mut(),
            );
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

    fn write_midi(&mut self, mut request: WriteMidiRequest) {
        unsafe {
            self.extended(
                EXT_WRITE_MIDI,
                &mut request as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
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
const EXT_WRITE_MIDI: i32 = 2359789;

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

fn detect_tempo(duration: DurationInSeconds, project: Option<Project>) -> Bpm {
    const MIN_BPM: f64 = 80.0;
    const MAX_BPM: f64 = 200.0;
    let project = project.unwrap_or_else(|| Reaper::get().current_project());
    let result = Reaper::get()
        .medium_reaper()
        .time_map_2_time_to_beats(project.context(), PositionInSeconds::ZERO);
    let numerator = result.time_signature.numerator;
    let mut bpm = numerator.get() as f64 * 60.0 / duration.get();
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
    }
    Bpm::new(bpm)
}

const FIXED_TEMPO_FACTOR: Option<f64> = None;
// const FIXED_TEMPO_FACTOR: Option<f64> = Some(1.0);
