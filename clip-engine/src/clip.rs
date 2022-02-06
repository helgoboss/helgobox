use crate::source_util::pcm_source_is_midi;
use crate::tempo_util::detect_tempo;
use crate::{
    adjust_proportionally_positive, clip_timeline, convert_duration_in_frames_to_other_frame_rate,
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    convert_position_in_frames_to_seconds, convert_position_in_seconds_to_frames,
    get_empty_midi_source, AudioBufMut, AudioSupplier, ClipContent, ClipRecordTiming,
    ClipSupplierChain, CreateClipContentMode, ExactDuration, ExactFrameCount, LegacyClip,
    LoopBehavior, MidiSupplier, RecordKind, SupplyAudioRequest, SupplyMidiRequest,
    SupplyRequestGeneralInfo, SupplyRequestInfo, Timeline, WithFrameRate, WithTempo,
    WriteAudioRequest, WriteMidiRequest,
};
use helgoboss_learn::UnitValue;
use reaper_high::{OwnedSource, Project, ReaperSource};
use reaper_low::raw::{midi_realtime_write_struct_t, PCM_SOURCE_EXT_ADDMIDIEVENTS};
use reaper_medium::{
    Bpm, DurationInSeconds, Hz, OwnedPcmSource, PcmSourceTransfer, PositionInSeconds,
    ReaperVolumeValue,
};
use std::path::PathBuf;
use std::ptr::null_mut;

#[derive(Debug)]
pub struct Clip {
    source_data: Option<SourceData>,
    supplier_chain: ClipSupplierChain,
    is_midi: bool,
    state: ClipState,
    repeated: bool,
    project: Option<Project>,
    // TODO-high Not yet implemented. This must also be implemented for MIDI, so we better choose
    //  a more neutral unit.
    volume: ReaperVolumeValue,
}

#[derive(Debug)]
struct SourceData {
    tempo: Bpm,
    beat_count: u32,
}

impl SourceData {
    fn from_source(source: &OwnedPcmSource, is_midi: bool, project: Option<Project>) -> Self {
        let tempo = source
            .tempo()
            .unwrap_or_else(|| detect_tempo(source.duration(), project));
        let beat_count = if is_midi {
            source
                .get_length_beats()
                .expect("MIDI source should report beats")
                .get()
                .round() as u32
        } else {
            let beats_per_sec = tempo.get() / 60.0;
            (source.duration().get() * beats_per_sec).round() as u32
        };
        assert_ne!(beat_count, 0, "source reported beat count of zero");
        Self { tempo, beat_count }
    }

    fn bar_count(&self) -> u32 {
        // TODO-high Respect different time signatures
        self.beat_count / 4
    }
}

#[derive(Copy, Clone, Debug)]
enum ClipState {
    /// At this state, the clip is stopped. No fade-in, no fade-out ... nothing.
    Stopped,
    Playing(PlayingState),
    /// Very short transition for fade outs or sending all-notes-off before entering another state.
    Suspending(SuspendingState),
    Paused(PausedState),
    /// Recording from scratch, not MIDI overdub.
    Recording(RecordingState),
}

#[derive(Copy, Clone, Debug, Default)]
struct PlayingState {
    pub start_bar: Option<i32>,
    pub pos: Option<InnerPos>,
    pub scheduled_for_stop: bool,
    pub overdubbing: bool,
    pub seek_pos: Option<usize>,
}

#[derive(Copy, Clone, Debug)]
struct SuspendingState {
    pub next_state: StateAfterSuspension,
    pub pos: InnerPos,
}

#[derive(Copy, Clone, Debug, Default)]
struct PausedState {
    pub pos: usize,
}

//region Description
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
//endregion
type InnerPos = isize;

#[derive(Copy, Clone, Debug)]
enum StateAfterSuspension {
    /// Play was suspended for initiating a retriggering, so the next state will be  
    /// [`ClipState::ScheduledOrPlaying`] again.
    Playing(PlayingState),
    /// Play was suspended for initiating a pause, so the next state will be [`ClipState::Paused`].
    Paused(PausedState),
    /// Play was suspended for initiating a stop, so the next state will be [`ClipState::Stopped`].
    Stopped,
    /// Play was suspended for initiating recording.
    Recording(RecordingState),
}

#[derive(Copy, Clone, Debug)]
struct RecordingState {
    /// Timeline position at which recording was triggered.
    pub timeline_start_pos: PositionInSeconds,
    /// Implies repeat.
    pub play_after: bool,
    pub timing: RecordTiming,
}

#[derive(Copy, Clone)]
pub enum RecordBehavior {
    Normal {
        play_after: bool,
        timing: RecordTiming,
    },
    MidiOverdub,
}

#[derive(Copy, Clone, Debug)]
pub enum RecordTiming {
    Unsynced,
    Synced {
        start_bar: i32,
        end_bar: Option<i32>,
    },
}

impl Clip {
    pub fn from_source(source: OwnedPcmSource, project: Option<Project>) -> Self {
        let is_midi = pcm_source_is_midi(&source);
        let source_data = SourceData::from_source(&source, is_midi, project);
        Self::new(
            Some(source_data),
            is_midi,
            ClipSupplierChain::new(source),
            project,
        )
    }

    pub fn empty(project: Option<Project>) -> Self {
        // TODO-high This should be None, ideally. We prepare the recording anyway.
        let source = get_empty_midi_source();
        Self::new(None, true, ClipSupplierChain::new(source), project)
    }

    fn new(
        source_data: Option<SourceData>,
        is_midi: bool,
        supplier_chain: ClipSupplierChain,
        project: Option<Project>,
    ) -> Self {
        Self {
            source_data,
            supplier_chain,
            is_midi,
            state: ClipState::Stopped,
            repeated: false,
            project,
            volume: Default::default(),
        }
    }

    pub fn play(&mut self, args: ClipPlayArgs) {
        use ClipState::*;
        match self.state {
            // Not yet running.
            Stopped => self.schedule_play_internal(args),
            Playing(s) => {
                if s.scheduled_for_stop {
                    // Scheduled for stop. Backpedal!
                    // We can only schedule for stop when repeated, so we can set this
                    // back to Infinitely.
                    self.supplier_chain
                        .looper_mut()
                        .set_loop_behavior(LoopBehavior::Infinitely);
                    self.state = Playing(PlayingState {
                        scheduled_for_stop: false,
                        ..s
                    });
                } else {
                    // Scheduled for play or playing already.
                    if let Some(pos) = s.pos {
                        if pos >= 0 {
                            // Already playing. Retrigger!
                            self.state = Suspending(SuspendingState {
                                next_state: StateAfterSuspension::Playing(PlayingState {
                                    start_bar: args.from_bar,
                                    ..Default::default()
                                }),
                                pos,
                            });
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
            Suspending(s) => {
                // It's important to handle this, otherwise some play actions simply have no effect,
                // which is especially annoying when using transport sync because then it's like
                // forgetting that clip ... the next time the transport is stopped and started,
                // that clip won't play again.
                self.state = ClipState::Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Playing(PlayingState {
                        start_bar: args.from_bar,
                        ..Default::default()
                    }),
                    ..s
                });
            }
            // TODO-high We should do a fade-in!
            Paused(s) => {
                // Resume
                self.state = ClipState::Playing(PlayingState {
                    pos: Some(s.pos as isize),
                    ..Default::default()
                });
            }
            Recording(_) => {
                // TODO-high It would probably be good to react the same way as if we would
                //  stop recording and press play right afterwards (or auto-play).
            }
        }
    }

    pub fn stop(&mut self, args: ClipStopArgs) {
        use ClipState::*;
        match self.state {
            Stopped => {}
            Playing(s) => {
                if s.overdubbing {
                    // Currently recording overdub. Stop recording, continue playing.
                    self.state = Playing(PlayingState {
                        overdubbing: false,
                        ..s
                    })
                } else {
                    // Just playing, not recording.
                    if let Some(pos) = s.pos {
                        if s.scheduled_for_stop {
                            // Already scheduled for stop.
                            if args.stop_behavior == ClipStopBehavior::Immediately {
                                // Transition to stop now!
                                self.state = Suspending(SuspendingState {
                                    next_state: StateAfterSuspension::Stopped,
                                    pos,
                                });
                            }
                        } else {
                            // Not yet scheduled for stop.
                            self.state = if pos >= 0 {
                                // Playing
                                match args.stop_behavior {
                                    ClipStopBehavior::Immediately => {
                                        // Immediately. Transition to stop.
                                        Suspending(SuspendingState {
                                            next_state: StateAfterSuspension::Stopped,
                                            pos,
                                        })
                                    }
                                    ClipStopBehavior::EndOfClip => {
                                        if self.repeated {
                                            // Schedule
                                            self.supplier_chain
                                                .looper_mut()
                                                .keep_playing_until_end_of_current_cycle(pos);
                                            Playing(PlayingState {
                                                scheduled_for_stop: true,
                                                ..s
                                            })
                                        } else {
                                            // Scheduling stop of a non-repeated clip doesn't make
                                            // sense.
                                            self.state
                                        }
                                    }
                                }
                            } else {
                                // Not yet playing. Backpedal.
                                Stopped
                            };
                        }
                    } else {
                        // Not yet playing. Backpedal.
                        self.state = Stopped;
                    }
                }
            }
            Paused(_) => {
                self.state = Stopped;
            }
            Suspending(s) => {
                if args.stop_behavior == ClipStopBehavior::Immediately {
                    // We are in another transition already. Simply change it to stop.
                    self.state = Suspending(SuspendingState {
                        next_state: StateAfterSuspension::Stopped,
                        ..s
                    });
                }
            }
            Recording(s) => {
                use RecordTiming::*;
                self.state = match s.timing {
                    Unsynced => {
                        if s.play_after {
                            Playing(Default::default())
                        } else {
                            Stopped
                        }
                    }
                    Synced { start_bar, end_bar } => {
                        // TODO-high If start bar in future, discard recording.
                        if end_bar.is_some() {
                            // End already scheduled. Take care of stopping after recording.
                            Recording(RecordingState {
                                play_after: false,
                                ..s
                            })
                        } else {
                            // End not scheduled yet. Schedule end.
                            let next_bar = args.timeline.next_bar_at(args.timeline_cursor_pos);
                            Recording(RecordingState {
                                timing: Synced {
                                    start_bar,
                                    end_bar: Some(next_bar),
                                },
                                ..s
                            })
                        }
                    }
                };
            }
        }
    }

    pub fn set_repeated(&mut self, repeated: bool) {
        self.repeated = repeated;
        let looper = self.supplier_chain.looper_mut();
        if !repeated {
            if let ClipState::Playing(PlayingState { pos: Some(pos), .. }) = self.state {
                looper.keep_playing_until_end_of_current_cycle(pos);
                return;
            }
        }
        looper.set_loop_behavior(LoopBehavior::from_bool(repeated));
    }

    pub fn repeated(&self) -> bool {
        self.repeated
    }

    pub fn volume(&self) -> ReaperVolumeValue {
        self.volume
    }

    pub fn midi_overdub(&mut self) {
        use ClipState::*;
        // TODO-medium Maybe we should start to play if not yet playing
        if let Playing(s) = self.state {
            self.state = Playing(PlayingState {
                overdubbing: true,
                ..s
            });
        }
    }

    pub fn record(&mut self, play_after: bool, timing: RecordTiming) {
        self.supplier_chain.recorder_mut().prepare_recording();
        use ClipState::*;
        let recording_state = RecordingState {
            timeline_start_pos: clip_timeline(self.project, false).cursor_pos(),
            play_after,
            timing,
        };
        self.state = match self.state {
            Stopped => Recording(recording_state),
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if pos >= 0 {
                        Suspending(SuspendingState {
                            next_state: StateAfterSuspension::Recording(recording_state),
                            pos,
                        })
                    } else {
                        Recording(recording_state)
                    }
                } else {
                    Recording(recording_state)
                }
            }
            Suspending(s) => Suspending(SuspendingState {
                next_state: StateAfterSuspension::Recording(recording_state),
                ..s
            }),
            Paused(_) | Recording(_) => Recording(recording_state),
        };
    }

    pub fn pause(&mut self) {
        use ClipState::*;
        match self.state {
            Stopped | Paused(_) => {}
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if pos >= 0 {
                        let pos = pos as usize;
                        // Playing. Pause!
                        // (If this clip is scheduled for stop already, a pause will backpedal from
                        // that.)
                        self.state = Suspending(SuspendingState {
                            next_state: StateAfterSuspension::Paused(PausedState { pos }),
                            pos: pos as isize,
                        });
                    }
                }
                // If not yet playing, we don't do anything at the moment.
                // TODO-medium In future, we could defer the clip scheduling to the future. I think
                //  that would feel natural.
            }
            Suspending(s) => {
                self.state = Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Paused(PausedState {
                        pos: s.pos as usize,
                    }),
                    ..s
                });
            }
            // Pausing recording ... no, we don't want that.
            Recording(_) => {}
        }
    }

    pub fn seek(&mut self, desired_pos: UnitValue) {
        let frame_count = self.supplier_chain.reaper_source().frame_count();
        let desired_frame = adjust_proportionally_positive(frame_count as f64, desired_pos.get());
        use ClipState::*;
        match self.state {
            Stopped | Suspending(_) | Recording(_) => {}
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if pos >= 0 {
                        let up_cycled_frame =
                            self.up_cycle_frame(desired_frame, pos as usize, frame_count);
                        self.state = Playing(PlayingState {
                            seek_pos: Some(up_cycled_frame),
                            ..s
                        });
                    }
                }
            }
            Paused(s) => {
                let up_cycled_frame = self.up_cycle_frame(desired_frame, s.pos, frame_count);
                self.state = Paused(PausedState {
                    pos: up_cycled_frame,
                });
            }
        }
    }

    fn up_cycle_frame(&self, frame: usize, offset_pos: usize, frame_count: usize) -> usize {
        let current_cycle = self.supplier_chain.looper().get_cycle_at_frame(offset_pos);
        current_cycle * frame_count + frame
    }

    pub fn record_source_type(&self) -> Option<ClipRecordSourceType> {
        use ClipState::*;
        match self.state {
            Stopped | Suspending(_) | Paused(_) => None,
            Playing(s) => {
                if s.overdubbing {
                    Some(ClipRecordSourceType::Midi)
                } else {
                    None
                }
            }
            // TODO-medium When recording past end, return None (should usually not happen)
            Recording(_) => {
                if self.is_midi {
                    Some(ClipRecordSourceType::Midi)
                } else {
                    Some(ClipRecordSourceType::Audio)
                }
            }
        }
    }

    pub fn write_midi(&mut self, request: WriteMidiRequest) {
        let timeline = clip_timeline(self.project, false);
        let timeline_cursor_pos = timeline.cursor_pos();
        use ClipState::*;
        let record_pos = match self.state {
            Playing(PlayingState {
                overdubbing: true, ..
            }) => self
                .position_in_seconds(timeline.tempo_at(timeline_cursor_pos))
                .unwrap_or_default(),
            Recording(s) => timeline_cursor_pos - s.timeline_start_pos,
            _ => return,
        };
        self.supplier_chain
            .recorder_mut()
            .write_midi(request, record_pos);
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        self.supplier_chain.recorder_mut().write_audio(request);
    }

    pub fn set_volume(&mut self, volume: ReaperVolumeValue) -> ClipChangedEvent {
        self.volume = volume;
        ClipChangedEvent::ClipVolume(volume)
    }

    pub fn info_legacy(&self) -> ClipInfo {
        let source = self.supplier_chain.reaper_source();
        ClipInfo {
            r#type: source.get_type(|t| t.to_string()),
            file_name: source.get_file_name(|p| Some(p?.to_owned())),
            length: {
                // TODO-low Doesn't need to be optional
                Some(source.duration())
            },
        }
    }

    pub fn descriptor_legacy(&self) -> LegacyClip {
        LegacyClip {
            volume: self.volume,
            repeat: self.repeated,
            content: Some(self.content()),
        }
    }

    fn content(&self) -> ClipContent {
        let source = self.supplier_chain.reaper_source();
        let source = ReaperSource::new(source.as_ptr());
        ClipContent::from_reaper_source(
            &source,
            CreateClipContentMode::AllowEmbeddedData,
            self.project,
        )
        .unwrap()
    }

    pub fn toggle_repeated(&mut self) -> ClipChangedEvent {
        self.set_repeated(!self.repeated);
        ClipChangedEvent::ClipRepeat(self.repeated)
    }

    pub fn play_state(&self) -> ClipPlayState {
        use ClipState::*;
        match self.state {
            Stopped => ClipPlayState::Stopped,
            Playing(s) => {
                if s.overdubbing {
                    ClipPlayState::Recording
                } else if s.scheduled_for_stop {
                    ClipPlayState::ScheduledForStop
                } else if let Some(pos) = s.pos {
                    if pos < 0 {
                        ClipPlayState::ScheduledForPlay
                    } else {
                        ClipPlayState::Playing
                    }
                } else {
                    ClipPlayState::ScheduledForPlay
                }
            }
            Suspending(s) => match s.next_state {
                StateAfterSuspension::Playing(_) => ClipPlayState::Playing,
                StateAfterSuspension::Paused(_) => ClipPlayState::Paused,
                StateAfterSuspension::Stopped => ClipPlayState::Stopped,
                StateAfterSuspension::Recording(_) => ClipPlayState::Recording,
            },
            Paused(_) => ClipPlayState::Paused,
            Recording(_) => ClipPlayState::Recording,
        }
    }

    pub fn position_in_seconds(&self, timeline_tempo: Bpm) -> Option<PositionInSeconds> {
        let source_pos_in_source_frames = self.frame_within_reaper_source()?;
        let source_pos_in_secs = convert_position_in_frames_to_seconds(
            source_pos_in_source_frames,
            self.supplier_chain.reaper_source().frame_rate(),
        );
        let final_tempo_factor = self.calc_final_tempo_factor(timeline_tempo);
        let source_pos_in_secs_tempo_adjusted = source_pos_in_secs.get() / final_tempo_factor;
        Some(PositionInSeconds::new(source_pos_in_secs_tempo_adjusted))
    }

    pub fn proportional_position(&self) -> Option<UnitValue> {
        let frame = self.frame_within_reaper_source()?;
        if frame < 0 {
            None
        } else {
            let frame_count = self.supplier_chain.reaper_source().frame_count();
            if frame_count == 0 {
                Some(UnitValue::MIN)
            } else {
                let proportional = UnitValue::new_clamped(frame as f64 / frame_count as f64);
                Some(proportional)
            }
        }
    }

    fn frame_within_reaper_source(&self) -> Option<isize> {
        use ClipState::*;
        match self.state {
            Playing(PlayingState { pos: Some(pos), .. })
            | Suspending(SuspendingState { pos, .. }) => {
                if pos < 0 {
                    Some(pos)
                } else {
                    Some(self.modulo_frame(pos as usize) as isize)
                }
            }
            // Pause position is modulo already.
            Paused(s) => Some(self.modulo_frame(s.pos) as isize),
            _ => None,
        }
    }

    fn modulo_frame(&self, frame: usize) -> usize {
        frame % self.supplier_chain.reaper_source().frame_count()
    }

    /// Returns if any samples could have been written.
    pub fn process(&mut self, args: ClipProcessArgs<impl Timeline>) {
        use ClipState::*;
        match self.state {
            Stopped | Paused(_) => {}
            Playing(s) => self.process_playing(s, args),
            Suspending(s) => self.process_suspending(s, args),
            Recording(s) => self.process_recording(s, args),
        }
    }

    fn process_playing(&mut self, s: PlayingState, args: ClipProcessArgs<impl Timeline>) {
        let general_info = self.prepare_playing(&args);
        struct Go {
            pos: isize,
            sample_rate_factor: f64,
            new_seek_pos: Option<usize>,
        }
        impl Default for Go {
            fn default() -> Self {
                Go {
                    pos: 0,
                    sample_rate_factor: 1.0,
                    new_seek_pos: None,
                }
            }
        }
        let go = if let Some(pos) = s.pos {
            // Already counting in or playing.
            if let Some(seek_pos) = s.seek_pos {
                // Seek requested.
                if self.is_midi {
                    // MIDI. Let's jump to the position directly.
                    Go {
                        pos: seek_pos as isize,
                        sample_rate_factor: 1.0,
                        new_seek_pos: None,
                    }
                } else {
                    // Audio. Let's fast-forward if possible.
                    let (sample_rate_factor, new_seek_pos) = if pos >= 0 {
                        // Playing.
                        let pos = pos as usize;
                        if pos < seek_pos {
                            // We might need to fast-forward.
                            let real_distance = seek_pos - pos;
                            let desired_distance_in_secs = DurationInSeconds::new(0.100);
                            let source_frame_rate =
                                self.supplier_chain.reaper_source().frame_rate();
                            let desired_distance = convert_duration_in_seconds_to_frames(
                                desired_distance_in_secs,
                                source_frame_rate,
                            );
                            if desired_distance < real_distance {
                                // We need to fast-forward.
                                let playback_speed_factor =
                                    32.0f64.min(real_distance as f64 / desired_distance as f64);
                                let sample_rate_factor = 1.0 / playback_speed_factor;
                                (sample_rate_factor, Some(seek_pos))
                            } else {
                                // We are almost there anyway, so no.
                                (1.0, None)
                            }
                        } else {
                            // We need to rewind. But we reject this at the moment.
                            (1.0, None)
                        }
                    } else {
                        // Counting in.
                        // We prevent seek during count-in but just in case, we reject it here.
                        (1.0, None)
                    };
                    Go {
                        pos,
                        sample_rate_factor,
                        new_seek_pos,
                    }
                }
            } else {
                // No seek requested
                Go {
                    pos,
                    ..Go::default()
                }
            }
        } else {
            // Not counting in or playing yet.
            let pos = if let Some(start_bar) = s.start_bar {
                // Scheduled play. Start countdown.
                self.calc_initial_pos_from_start_bar(
                    start_bar,
                    &args,
                    general_info.clip_tempo_factor,
                )
            } else {
                // Immediate play.
                0
            };
            Go {
                pos,
                ..Go::default()
            }
        };
        self.state = if let Some(end_frame) =
            self.fill_samples(args, go.pos, &general_info, go.sample_rate_factor)
        {
            // There's still something to play.
            ClipState::Playing(PlayingState {
                pos: Some(end_frame),
                seek_pos: go.new_seek_pos.and_then(|new_seek_pos| {
                    // Check if we reached our desired position.
                    if end_frame >= new_seek_pos as isize {
                        // Reached
                        None
                    } else {
                        // Not reached yet.
                        Some(new_seek_pos)
                    }
                }),
                ..s
            })
        } else {
            // We have reached the natural or scheduled end. Everything that needed to be
            // played has been played in previous blocks. Audio fade outs have been applied
            // as well, so no need to go to suspending state first. Go right to stop!
            self.reset_for_play();
            ClipState::Stopped
        };
    }

    fn reset_for_play(&mut self) {
        self.supplier_chain.suspender_mut().reset();
        self.supplier_chain
            .resampler_mut()
            .reset_buffers_and_latency();
        self.supplier_chain
            .time_stretcher_mut()
            .reset_buffers_and_latency();
        self.supplier_chain
            .looper_mut()
            .set_loop_behavior(LoopBehavior::from_bool(self.repeated));
    }

    fn fill_samples(
        &mut self,
        args: ClipProcessArgs<impl Timeline>,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        sample_rate_factor: f64,
    ) -> Option<isize> {
        let dest_sample_rate = Hz::new(args.block.sample_rate().get() * sample_rate_factor);
        if self.is_midi {
            self.fill_samples_midi(args, start_frame, info, dest_sample_rate)
        } else {
            self.fill_samples_audio(args, start_frame, info, dest_sample_rate)
        }
    }

    fn fill_samples_audio(
        &mut self,
        args: ClipProcessArgs<impl Timeline>,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        dest_sample_rate: Hz,
    ) -> Option<isize> {
        let request = SupplyAudioRequest {
            start_frame,
            dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-audio",
                note: "",
            },
            parent_request: None,
            general_info: info,
        };
        let mut dest_buffer = unsafe {
            AudioBufMut::from_raw(
                args.block.samples(),
                args.block.nch() as _,
                args.block.length() as _,
            )
        };
        let response = self
            .supplier_chain
            .head_mut()
            .supply_audio(&request, &mut dest_buffer);
        // TODO-high There's an issue e.g. when playing the piano audio clip that makes
        //  the clip not stop for a long time when it's not looped. Check that!
        response.next_inner_frame
    }

    fn fill_samples_midi(
        &mut self,
        args: ClipProcessArgs<impl Timeline>,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        dest_sample_rate: Hz,
    ) -> Option<isize> {
        let request = SupplyMidiRequest {
            start_frame,
            dest_frame_count: args.block.length() as _,
            dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-midi",
                note: "",
            },
            parent_request: None,
            general_info: info,
        };
        let response = self.supplier_chain.head_mut().supply_midi(
            &request,
            args.block.midi_event_list().expect("no MIDI event list"),
        );
        response.next_inner_frame
    }

    /// So, this is how we do play scheduling. Whenever the preview register
    /// calls get_samples() and we are in a fresh ScheduledOrPlaying state, the
    /// relative number of count-in frames will be determined. Based on the given
    /// absolute bar for which the clip is scheduled.
    ///
    /// 1. We use a *relative* count-in (instead of just
    /// using the absolute scheduled-play position and check if we reached it)
    /// in order to respect arbitrary tempo changes during the count-in phase and
    /// still end up starting on the correct point in time. Okay, we could reach
    /// the same goal also by regularly checking whether we finally reached the
    /// start of the bar. But first, we need the relative count-in later anyway
    /// (for pickup beats, which start to play during count-in time). And second,
    /// it would be also pretty much unnecessary beat-time mapping.
    ///
    /// 2. We resolve the
    /// count-in length here in the real-time context, not before! In particular not
    /// at the time the play is requested. At that time we just calculate the
    /// bar index. Reason: The start time of the next bar at play-request time
    /// is not necessarily the same as the one in the get_samples call. If it's not,
    /// we would start advancing the count-in cursor from a wrong initial state
    /// and therefore end up with the wrong point in time for starting the clip
    /// (too late, to be accurate, because we would start advancing too late).
    fn calc_initial_pos_from_start_bar(
        &self,
        start_bar: i32,
        args: &ClipProcessArgs<impl Timeline>,
        clip_tempo_factor: f64,
    ) -> isize {
        // Basics
        let block_length_in_timeline_frames = args.block.length() as usize;
        let source_frame_rate = self.supplier_chain.reaper_source().frame_rate();
        let timeline_frame_rate = args.block.sample_rate();
        // Essential calculation
        let start_bar_timeline_pos = args.timeline.pos_of_bar(start_bar);
        let rel_pos_from_bar_in_secs = args.timeline_cursor_pos - start_bar_timeline_pos;
        let rel_pos_from_bar_in_source_frames = convert_position_in_seconds_to_frames(
            rel_pos_from_bar_in_secs,
            self.supplier_chain.reaper_source().frame_rate(),
        );
        {
            let args = LogNaturalDeviationArgs {
                start_bar,
                block: args.block,
                timeline: &args.timeline,
                timeline_cursor_pos: args.timeline_cursor_pos,
                clip_tempo_factor,
                timeline_frame_rate,
                source_frame_rate,
                start_bar_timeline_pos,
            };
            self.log_natural_deviation(args);
        }
        //region Description
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
        //endregion
        let block_length_in_source_frames = convert_duration_in_frames_to_other_frame_rate(
            block_length_in_timeline_frames,
            timeline_frame_rate,
            source_frame_rate,
        );
        adjust_proportionally_in_blocks(
            rel_pos_from_bar_in_source_frames,
            clip_tempo_factor,
            block_length_in_source_frames,
        )
    }

    fn log_natural_deviation(&self, args: LogNaturalDeviationArgs<impl Timeline>) {
        // Assuming a constant tempo and time signature during one cycle
        // TODO-high Just temporary! We should introduce 2 parent states: Recording and Ready.
        //  Ready has source data and contains all the known states such as Playing, Stopped, ...
        //  In Recording state, the reaper_source() should be None.
        let bar_count = self
            .source_data
            .as_ref()
            .map(|d| d.bar_count())
            .unwrap_or(1);
        let end_bar = args.start_bar + bar_count as i32;
        let bar_count = end_bar - args.start_bar;
        let end_bar_timeline_pos = args.timeline.pos_of_bar(end_bar);
        assert!(end_bar_timeline_pos > args.start_bar_timeline_pos);
        // Timeline cycle length
        let timeline_cycle_length_in_secs =
            (end_bar_timeline_pos - args.start_bar_timeline_pos).abs();
        let timeline_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
            timeline_cycle_length_in_secs,
            args.timeline_frame_rate,
        );
        let timeline_cycle_length_in_source_frames = convert_duration_in_seconds_to_frames(
            timeline_cycle_length_in_secs,
            args.source_frame_rate,
        );
        // Source cycle length
        let source_cycle_length_in_secs = self.supplier_chain.reaper_source().duration();
        let source_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
            source_cycle_length_in_secs,
            args.timeline_frame_rate,
        );
        let source_cycle_length_in_source_frames =
            self.supplier_chain.reaper_source().frame_count();
        // Block length
        let block_length_in_timeline_frames = args.block.length() as usize;
        let block_length_in_secs = convert_duration_in_frames_to_seconds(
            block_length_in_timeline_frames,
            args.timeline_frame_rate,
        );
        let block_length_in_source_frames = convert_duration_in_frames_to_other_frame_rate(
            block_length_in_timeline_frames,
            args.timeline_frame_rate,
            args.source_frame_rate,
        );
        // Block count and remainder
        let num_full_blocks = source_cycle_length_in_source_frames / block_length_in_source_frames;
        let remainder_in_source_frames =
            source_cycle_length_in_source_frames % block_length_in_source_frames;
        // Tempo-adjusted
        let adjusted_block_length_in_source_frames = adjust_proportionally_positive(
            block_length_in_source_frames as f64,
            args.clip_tempo_factor,
        );
        let adjusted_block_length_in_timeline_frames =
            convert_duration_in_frames_to_other_frame_rate(
                adjusted_block_length_in_source_frames,
                args.source_frame_rate,
                args.timeline_frame_rate,
            );
        let adjusted_block_length_in_secs = convert_duration_in_frames_to_seconds(
            adjusted_block_length_in_source_frames,
            args.source_frame_rate,
        );
        let adjusted_remainder_in_source_frames = adjust_proportionally_positive(
            remainder_in_source_frames as f64,
            args.clip_tempo_factor,
        );
        // Source cycle remainder
        let adjusted_remainder_in_timeline_frames = convert_duration_in_frames_to_other_frame_rate(
            adjusted_remainder_in_source_frames,
            args.source_frame_rate,
            args.timeline_frame_rate,
        );
        let adjusted_remainder_in_secs = convert_duration_in_frames_to_seconds(
            adjusted_remainder_in_source_frames,
            args.source_frame_rate,
        );
        let block_index = (args.timeline_cursor_pos.get() / block_length_in_secs.get()) as isize;
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
            args.clip_tempo_factor,
            bar_count,
            args.start_bar,
            end_bar,
            args.start_bar_timeline_pos.get(),
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

    fn process_suspending(&mut self, s: SuspendingState, args: ClipProcessArgs<impl Timeline>) {
        let general_info = self.prepare_playing(&args);
        let suspender = self.supplier_chain.suspender_mut();
        if !suspender.is_suspending() {
            suspender.suspend(s.pos);
        }
        self.state = if let Some(end_frame) = self.fill_samples(args, s.pos, &general_info, 1.0) {
            // Suspension not finished yet.
            ClipState::Suspending(SuspendingState {
                pos: end_frame,
                ..s
            })
        } else {
            // Suspension finished.
            use StateAfterSuspension::*;
            match s.next_state {
                Playing(s) => ClipState::Playing(s),
                Paused(s) => {
                    // TODO-high Set follow-up Pause state in pause() correctly, see old
                    //  get_suspension_follow_up_state()
                    ClipState::Paused(s)
                }
                Stopped => {
                    self.reset_for_play();
                    ClipState::Stopped
                }
                Recording(s) => ClipState::Recording(s),
            }
        };
    }

    fn process_recording(&mut self, s: RecordingState, args: ClipProcessArgs<impl Timeline>) {
        if let RecordTiming::Synced {
            end_bar: Some(end_bar),
            ..
        } = s.timing
        {
            if args.timeline.next_bar_at(args.timeline_cursor_pos) >= end_bar {
                // Close to scheduled recording end.
                let block_length_in_timeline_frames = args.block.length() as usize;
                let timeline_frame_rate = args.block.sample_rate();
                let block_length_in_secs = convert_duration_in_frames_to_seconds(
                    block_length_in_timeline_frames,
                    timeline_frame_rate,
                );
                let block_end_pos = args.timeline_cursor_pos + block_length_in_secs;
                let record_end_pos = args.timeline.pos_of_bar(end_bar);
                if block_end_pos >= record_end_pos {
                    // We have recorded the last block.
                    if s.play_after {
                        self.set_repeated(true);
                        self.state = ClipState::Playing(PlayingState {
                            start_bar: Some(end_bar),
                            ..Default::default()
                        });
                        self.process(args);
                    } else {
                        self.state = ClipState::Stopped;
                    }
                }
            }
        }
    }

    fn prepare_playing(
        &mut self,
        args: &ClipProcessArgs<impl Timeline>,
    ) -> SupplyRequestGeneralInfo {
        let final_tempo_factor = self.calc_final_tempo_factor(args.timeline_tempo);
        let general_info = SupplyRequestGeneralInfo {
            audio_block_timeline_cursor_pos: args.timeline_cursor_pos,
            audio_block_length: args.block.length() as usize,
            output_frame_rate: args.block.sample_rate(),
            timeline_tempo: args.timeline_tempo,
            clip_tempo_factor: final_tempo_factor,
        };
        self.supplier_chain
            .time_stretcher_mut()
            .set_tempo_factor(final_tempo_factor);
        general_info
    }

    fn schedule_play_internal(&mut self, args: ClipPlayArgs) {
        self.state = ClipState::Playing(PlayingState {
            start_bar: args.from_bar,
            ..Default::default()
        });
    }

    fn calc_final_tempo_factor(&self, timeline_tempo: Bpm) -> f64 {
        if let Some(d) = &self.source_data {
            let timeline_tempo_factor = timeline_tempo.get() / d.tempo.get();
            timeline_tempo_factor.max(MIN_TEMPO_FACTOR)
        } else {
            1.0
        }
    }
}

/// It can make a difference if we apply a factor once on a large integer x and then round or
/// n times on x/n and round each time. Latter is what happens in practice because we advance
/// frames step by step in n blocks.
fn adjust_proportionally_in_blocks(value: isize, factor: f64, block_length: usize) -> isize {
    let abs_value = value.abs() as usize;
    let block_count = abs_value / block_length;
    let remainder = abs_value % block_length;
    let adjusted_block_length = adjust_proportionally_positive(block_length as f64, factor);
    let adjusted_remainder = adjust_proportionally_positive(remainder as f64, factor);
    let total_without_remainder = block_count * adjusted_block_length;
    let total = total_without_remainder + adjusted_remainder;
    // dbg!(abs_value, adjusted_block_length, block_count, remainder, adjusted_remainder, total_without_remainder, total);
    total as isize * value.signum()
}

pub struct ClipPlayArgs {
    pub from_bar: Option<i32>,
}

pub struct ClipStopArgs<'a> {
    pub stop_behavior: ClipStopBehavior,
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline: &'a dyn Timeline,
}

#[derive(PartialEq)]
pub enum ClipStopBehavior {
    Immediately,
    EndOfClip,
}

pub struct ClipProcessArgs<'a, T: Timeline> {
    pub block: &'a mut PcmSourceTransfer,
    pub timeline: T,
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
}

struct LogNaturalDeviationArgs<'a, T: Timeline> {
    start_bar: i32,
    block: &'a PcmSourceTransfer,
    timeline: T,
    timeline_cursor_pos: PositionInSeconds,
    // timeline_tempo: Bpm,
    clip_tempo_factor: f64,
    timeline_frame_rate: Hz,
    source_frame_rate: Hz,
    start_bar_timeline_pos: PositionInSeconds,
}

const MIN_TEMPO_FACTOR: f64 = 0.0000000001;

#[derive(Debug)]
pub enum ClipRecordSourceType {
    Midi,
    Audio,
}

/// Contains static information about a clip.
pub struct ClipInfo {
    pub r#type: String,
    pub file_name: Option<PathBuf>,
    pub length: Option<DurationInSeconds>,
}

/// Play state of a clip.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ClipPlayState {
    Stopped,
    ScheduledForPlay,
    Playing,
    Paused,
    ScheduledForStop,
    Recording,
}

impl ClipPlayState {
    /// Translates this play state into a feedback value.
    pub fn feedback_value(self) -> UnitValue {
        use ClipPlayState::*;
        match self {
            Stopped => UnitValue::MIN,
            ScheduledForPlay => UnitValue::new(0.75),
            Playing => UnitValue::MAX,
            Paused => UnitValue::new(0.5),
            ScheduledForStop => UnitValue::new(0.25),
            Recording => UnitValue::new(0.60),
        }
    }
}

impl Default for ClipPlayState {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug)]
pub enum ClipChangedEvent {
    PlayState(ClipPlayState),
    ClipVolume(ReaperVolumeValue),
    ClipRepeat(bool),
    ClipPosition(UnitValue),
}
