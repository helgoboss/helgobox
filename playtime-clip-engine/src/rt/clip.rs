use crate::conversion_util::{
    adjust_pos_in_secs_anti_proportionally, adjust_proportionally_positive,
    convert_duration_in_frames_to_other_frame_rate, convert_duration_in_frames_to_seconds,
    convert_duration_in_seconds_to_frames, convert_position_in_frames_to_seconds,
    convert_position_in_seconds_to_frames,
};
use crate::main::{create_pcm_source_from_api_source, ClipSlotCoordinates};
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, MidiSupplier, PreBufferFillRequest, PreBufferRequest, PreBufferSourceSkill,
    Recorder, RecorderEquipment, SupplierChain, SupplyAudioRequest, SupplyMidiRequest,
    SupplyRequestGeneralInfo, SupplyRequestInfo, SupplyResponse, SupplyResponseStatus,
    WithMaterialInfo, WriteAudioRequest, WriteMidiRequest, MIDI_BASE_BPM,
};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline};
use crate::{ClipEngineResult, QuantizedPosition};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, BeatTimeBase, ClipPlayStartTiming,
    ClipPlayStopTiming, ClipTimeBase, Db, EvenQuantization, VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, PositionInSeconds};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct Clip {
    supplier_chain: SupplierChain,
    state: ClipState,
    project: Option<Project>,
    shared_pos: SharedPos,
}

#[derive(Copy, Clone, Debug)]
struct PersistentPlayData {
    start_timing: Option<ClipPlayStartTiming>,
    stop_timing: Option<ClipPlayStopTiming>,
    looped: bool,
    time_base: ClipTimeBase,
}

fn calculate_beat_count(tempo: Bpm, duration: DurationInSeconds) -> u32 {
    let beats_per_sec = tempo.get() / 60.0;
    (duration.get() * beats_per_sec).round() as u32
}

#[derive(Copy, Clone, Debug)]
enum ClipState {
    Ready(ReadyState),
    /// Recording from scratch, not MIDI overdub.
    Recording(RecordingState),
}

impl ClipState {
    fn is_playing(&self) -> bool {
        use ClipState::*;
        matches!(
            self,
            Ready(ReadyState {
                state: ReadySubState::Playing(_),
                ..
            })
        )
    }
}

#[derive(Copy, Clone, Debug)]
struct ReadyState {
    state: ReadySubState,
    // TODO-low If this gets too big, we might run into a performance issue and we should make it
    //  non-copy.
    persistent_data: PersistentPlayData,
}

#[derive(Copy, Clone, Debug)]
enum ReadySubState {
    /// At this state, the clip is stopped. No fade-in, no fade-out ... nothing.
    Stopped,
    Playing(PlayingState),
    /// Very short transition for fade outs or sending all-notes-off before entering another state.
    Suspending(SuspendingState),
    Paused(PausedState),
}

#[derive(Copy, Clone, Debug, Default)]
struct PlayingState {
    pub virtual_pos: VirtualPosition,
    /// Position within material, not a timeline position.
    pub pos: Option<MaterialPos>,
    pub stop_request: Option<StopRequest>,
    pub overdubbing: bool,
    pub seek_pos: Option<usize>,
}

#[derive(Copy, Clone, Debug)]
enum StopRequest {
    AtEndOfClip,
    Quantized(QuantizedPosition),
}

#[derive(Copy, Clone, Debug)]
struct SuspendingState {
    pub next_state: StateAfterSuspension,
    pub pos: MaterialPos,
}

#[derive(Copy, Clone, Debug, Default)]
struct PausedState {
    pub pos: MaterialPos,
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
type MaterialPos = isize;

#[derive(Clone, Debug, Default)]
pub struct SharedPos(Arc<AtomicIsize>);

impl SharedPos {
    pub fn get(&self) -> MaterialPos {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, pos: isize) {
        self.0.store(pos, Ordering::Relaxed);
    }
}

#[derive(Copy, Clone, Debug)]
enum StateAfterSuspension {
    /// Play was suspended for initiating a retriggering, so the next state will be  
    /// [`ClipState::ScheduledOrPlaying`] again.
    Playing(PlayingState),
    /// Play was suspended for initiating a pause, so the next state will be [`ClipState::Paused`].
    Paused,
    /// Play was suspended for initiating a stop, so the next state will be [`ClipState::Stopped`].
    Stopped,
    /// Play was suspended for initiating recording.
    Recording(RecordingState),
}

#[derive(Copy, Clone, Debug)]
struct RecordingState {
    /// Timeline position at which recording was triggered.
    pub trigger_timeline_pos: PositionInSeconds,
    /// Implies play-after-record.
    pub looped: bool,
    pub timing: RecordTiming,
    pub input: ClipRecordInput,
    pub rollback_data: Option<RollbackData>,
}

#[derive(Copy, Clone, Debug)]
struct RollbackData {
    persistent_data: PersistentPlayData,
}

#[derive(Copy, Clone)]
pub enum RecordBehavior {
    Normal {
        looped: bool,
        timing: RecordTiming,
        detect_downbeat: bool,
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
    pub fn ready(
        api_clip: &api::Clip,
        permanent_project: Option<Project>,
        recorder_equipment: &RecorderEquipment,
        pre_buffer_request_sender: &Sender<PreBufferRequest>,
    ) -> ClipEngineResult<Self> {
        let pcm_source = create_pcm_source_from_api_source(&api_clip.source, permanent_project)?;
        let mut ready_state = ReadyState {
            state: ReadySubState::Stopped,
            persistent_data: PersistentPlayData {
                start_timing: api_clip.start_timing,
                stop_timing: api_clip.stop_timing,
                looped: api_clip.looped,
                time_base: api_clip.time_base,
            },
        };
        let mut supplier_chain = SupplierChain::new(
            Recorder::ready(pcm_source, recorder_equipment.clone()),
            pre_buffer_request_sender.clone(),
        );
        supplier_chain.set_volume(api_clip.volume);
        supplier_chain
            .set_section_bounds_in_seconds(api_clip.section.start_pos, api_clip.section.length)?;
        supplier_chain.set_midi_reset_msg_range_for_interaction(
            api_clip.midi_settings.interaction_reset_settings,
        );
        supplier_chain
            .set_midi_reset_msg_range_for_source(api_clip.midi_settings.source_reset_settings);
        supplier_chain
            .set_midi_reset_msg_range_for_section(api_clip.midi_settings.section_reset_settings);
        supplier_chain
            .set_midi_reset_msg_range_for_loop(api_clip.midi_settings.loop_reset_settings);
        supplier_chain
            .set_audio_fades_enabled_for_source(api_clip.audio_settings.apply_source_fades);
        ready_state.update_supplier_chain_from_persistent_data(&mut supplier_chain)?;
        ready_state.pre_buffer(&mut supplier_chain, 0);
        let clip = Self {
            supplier_chain,
            state: ClipState::Ready(ready_state),
            project: permanent_project,
            shared_pos: Default::default(),
        };
        Ok(clip)
    }

    pub fn recording(
        args: ClipRecordArgs,
        project: Option<Project>,
        equipment: RecorderEquipment,
        pre_buffer_request_sender: Sender<PreBufferRequest>,
    ) -> Self {
        let timeline = clip_timeline(project, false);
        let trigger_timeline_pos = timeline.cursor_pos();
        let tempo = timeline.tempo_at(trigger_timeline_pos);
        let recording_state = RecordingState {
            trigger_timeline_pos,
            looped: args.looped,
            timing: args.timing,
            input: args.input,
            rollback_data: None,
        };
        let recorder = Recorder::recording(
            args.input,
            project,
            trigger_timeline_pos,
            tempo,
            equipment,
            args.detect_downbeat,
            args.timing,
        );
        Self {
            supplier_chain: SupplierChain::new(recorder, pre_buffer_request_sender),
            state: ClipState::Recording(recording_state),
            project,
            shared_pos: Default::default(),
        }
    }

    pub fn set_audio_resample_mode(&mut self, mode: VirtualResampleMode) {
        self.supplier_chain.set_audio_resample_mode(mode);
    }

    pub fn set_audio_time_stretch_mode(&mut self, mode: AudioTimeStretchMode) {
        self.supplier_chain.set_audio_time_stretch_mode(mode);
    }

    pub fn set_audio_cache_behavior(&mut self, cache_behavior: AudioCacheBehavior) {
        self.supplier_chain
            .set_audio_cache_behavior(cache_behavior)
            .unwrap();
    }

    /// Plays the clip if it's not recording.
    pub fn play(&mut self, args: ClipPlayArgs) -> ClipEngineResult<PlayOutcome> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => Ok(s.play(args, &mut self.supplier_chain)),
            Recording(_) => Err("recording"),
        }
    }

    /// Stops the clip playing or recording.
    #[must_use]
    pub fn stop(&mut self, args: ClipStopArgs) -> SlotInstruction {
        use ClipState::*;
        use RecordingStopOutcome::*;
        let outcome = match &mut self.state {
            Ready(s) => {
                s.stop(args, &mut self.supplier_chain);
                KeepState
            }
            Recording(s) => s.stop(args, &mut self.supplier_chain),
        };
        match outcome {
            KeepState => SlotInstruction::KeepSlot,
            TransitionToReady(ready_state) => {
                self.state = Ready(ready_state);
                SlotInstruction::KeepSlot
            }
            ClearSlot => SlotInstruction::ClearSlot,
        }
    }

    pub fn set_looped(&mut self, looped: bool) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                s.set_looped(looped, &mut self.supplier_chain);
            }
            Recording(s) => s.set_looped(looped),
        }
    }

    pub fn looped(&self) -> bool {
        use ClipState::*;
        match self.state {
            Ready(s) => s.persistent_data.looped,
            Recording(s) => s.looped,
        }
    }

    pub fn midi_overdub(&mut self) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.midi_overdub(),
            Recording(_) => {}
        }
    }

    pub fn record(&mut self, args: ClipRecordArgs) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                if let Some(recording_state) =
                    s.record(args, self.project, &mut self.supplier_chain)
                {
                    self.state = Recording(recording_state);
                }
            }
            Recording(_) => {}
        }
    }

    pub fn pause(&mut self) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.pause(&self.supplier_chain),
            Recording(_) => {}
        }
    }

    pub fn seek(&mut self, desired_pos: UnitValue) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.seek(desired_pos, &self.supplier_chain),
            Recording(_) => {}
        }
    }

    pub fn record_input(&self) -> Option<ClipRecordInput> {
        use ClipState::*;
        match &self.state {
            Ready(s) => {
                use ReadySubState::*;
                match s.state {
                    Stopped | Suspending(_) | Paused(_) => None,
                    Playing(s) => {
                        if s.overdubbing {
                            Some(ClipRecordInput::Midi)
                        } else {
                            None
                        }
                    }
                }
            }
            // TODO-medium When recording past end, return None (should usually not happen)
            Recording(s) => Some(s.input),
        }
    }

    pub fn write_midi(&mut self, request: WriteMidiRequest) {
        let timeline = clip_timeline(self.project, false);
        let timeline_cursor_pos = timeline.cursor_pos();
        use ClipState::*;
        let record_pos = match &self.state {
            Ready(s) => {
                use ReadySubState::*;
                if let Playing(PlayingState {
                    overdubbing: true, ..
                }) = s.state
                {
                    self.position_in_seconds(timeline.tempo_at(timeline_cursor_pos))
                        .unwrap_or_default()
                } else {
                    return;
                }
            }
            // TODO-high-record Depending on the trigger timeline pos is not good with tempo changes.
            Recording(s) => timeline_cursor_pos - s.trigger_timeline_pos,
        };
        if record_pos < PositionInSeconds::ZERO {
            return;
        }
        self.supplier_chain
            .write_midi(request, DurationInSeconds::new(record_pos.get()));
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        self.supplier_chain.write_audio(request);
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.supplier_chain.set_volume(volume);
    }

    pub fn shared_pos(&self) -> SharedPos {
        self.shared_pos.clone()
    }

    pub fn source_frame_rate(&self) -> Hz {
        self.supplier_chain.source_frame_rate_in_ready_state()
    }

    pub fn is_midi(&self) -> bool {
        self.supplier_chain.is_midi()
    }

    pub fn channel_count(&self) -> usize {
        self.supplier_chain
            .material_info()
            .map(|i| i.channel_count())
            .unwrap_or(0)
    }

    pub fn play_state(&self) -> ClipPlayState {
        use ClipState::*;
        match &self.state {
            Ready(s) => s.play_state(),
            Recording(_) => ClipPlayState::Recording,
        }
    }

    pub fn position_in_seconds(&self, timeline_tempo: Bpm) -> Option<PositionInSeconds> {
        use ClipState::*;
        match &self.state {
            Ready(s) => Some(s.position_in_seconds(timeline_tempo, &self.supplier_chain)),
            Recording(_) => None,
        }
    }

    pub fn effective_frame_count(&self) -> usize {
        self.supplier_chain.section_frame_count_in_ready_state()
    }

    pub fn process(&mut self, args: &mut ClipProcessArgs) -> ClipPlayingOutcome {
        use ClipState::*;
        let (outcome, changed_state) = match &mut self.state {
            Ready(s) => {
                let (outcome, changed_state) =
                    s.process(args, &mut self.supplier_chain, &mut self.shared_pos);
                (Some(outcome), changed_state.map(Recording))
            }
            Recording(s) => {
                let changed_state = s.process(args, &mut self.supplier_chain);
                (None, changed_state.map(Ready))
            }
        };
        let outcome = if let Some(s) = changed_state {
            self.state = s;
            if s.is_playing() {
                // Changed from record to playing. Don't miss any samples!
                Some(self.process(args))
            } else {
                outcome
            }
        } else {
            outcome
        };
        outcome.unwrap_or_default()
    }
}

impl ReadyState {
    pub fn position_in_seconds(
        &self,
        timeline_tempo: Bpm,
        supplier_chain: &SupplierChain,
    ) -> PositionInSeconds {
        let source_pos_in_source_frames = self.frame_within_reaper_source(supplier_chain);
        let source_pos_in_secs = convert_position_in_frames_to_seconds(
            source_pos_in_source_frames,
            supplier_chain.source_frame_rate_in_ready_state(),
        );
        let tempo_factor = self.calc_tempo_factor(timeline_tempo, supplier_chain.is_midi());
        adjust_pos_in_secs_anti_proportionally(source_pos_in_secs, tempo_factor)
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.persistent_data.time_base, is_midi)
    }

    fn frame_within_reaper_source(&self, supplier_chain: &SupplierChain) -> isize {
        use ReadySubState::*;
        let absolute_frame = match self.state {
            Playing(PlayingState {
                seek_pos: Some(pos),
                ..
            }) => pos as isize,
            Playing(PlayingState { pos: Some(pos), .. })
            | Suspending(SuspendingState { pos, .. }) => pos,
            // Pause position is modulo already.
            Paused(s) => s.pos as isize,
            _ => return 0,
        };
        self.modulo_frame(absolute_frame, supplier_chain)
    }

    fn modulo_frame(&self, frame: isize, supplier_chain: &SupplierChain) -> isize {
        if frame < 0 {
            frame
        } else {
            frame % supplier_chain.section_frame_count_in_ready_state() as isize
        }
    }

    pub fn set_looped(&mut self, looped: bool, supplier_chain: &mut SupplierChain) {
        self.persistent_data.looped = looped;
        if !looped {
            if let ReadySubState::Playing(PlayingState { pos: Some(pos), .. }) = self.state {
                supplier_chain.keep_playing_until_end_of_current_cycle(pos);
                return;
            }
        }
        supplier_chain.set_looped(self.persistent_data.looped);
    }

    fn update_supplier_chain_from_persistent_data(
        &self,
        supplier_chain: &mut SupplierChain,
    ) -> ClipEngineResult<()> {
        supplier_chain.set_looped(self.persistent_data.looped);
        match &self.persistent_data.time_base {
            ClipTimeBase::Time => {
                supplier_chain.set_time_stretching_enabled(false);
                supplier_chain.clear_downbeat();
            }
            ClipTimeBase::Beat(b) => {
                supplier_chain.set_time_stretching_enabled(true);
                let tempo = determine_tempo_from_beat_time_base(b, supplier_chain.is_midi());
                supplier_chain.set_downbeat_in_beats(b.downbeat, tempo)?;
            }
        }
        Ok(())
    }

    pub fn play(&mut self, args: ClipPlayArgs, supplier_chain: &mut SupplierChain) -> PlayOutcome {
        let virtual_pos = self.calculate_virtual_play_pos(&args);
        use ReadySubState::*;
        match self.state {
            // Not yet running.
            Stopped => self.schedule_play_internal(virtual_pos),
            Playing(s) => {
                if s.stop_request.is_some() {
                    // Scheduled for stop. Backpedal!
                    // We can only schedule for stop when repeated, so we can set this
                    // back to Infinitely.
                    supplier_chain.set_looped(true);
                    // If we have a quantized stop, the interaction handler is active. Clear!
                    supplier_chain.reset_interactions();
                    self.state = Playing(PlayingState {
                        stop_request: None,
                        ..s
                    });
                } else {
                    // Scheduled for play or playing already.
                    if let Some(pos) = s.pos {
                        if supplier_chain.is_playing_already(pos) {
                            // Already playing. Retrigger!
                            self.state = Suspending(SuspendingState {
                                next_state: StateAfterSuspension::Playing(PlayingState {
                                    virtual_pos,
                                    ..Default::default()
                                }),
                                pos,
                            });
                        } else {
                            // Not yet playing. Reschedule!
                            self.schedule_play_internal(virtual_pos);
                        }
                    } else {
                        // Not yet playing. Reschedule!
                        self.schedule_play_internal(virtual_pos);
                    }
                }
            }
            Suspending(s) => {
                // It's important to handle this, otherwise some play actions simply have no effect,
                // which is especially annoying when using transport sync because then it's like
                // forgetting that clip ... the next time the transport is stopped and started,
                // that clip won't play again.
                self.state = ReadySubState::Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Playing(PlayingState {
                        virtual_pos,
                        ..Default::default()
                    }),
                    ..s
                });
            }
            Paused(s) => {
                // Resume
                let pos = s.pos as isize;
                supplier_chain.install_immediate_start_interaction(pos);
                self.state = ReadySubState::Playing(PlayingState {
                    pos: Some(pos),
                    ..Default::default()
                });
            }
        }
        PlayOutcome { virtual_pos }
    }

    fn resolve_stop_timing(&self, stop_args: &ClipStopArgs) -> ConcreteClipPlayStopTiming {
        let start_timing = self
            .persistent_data
            .start_timing
            .unwrap_or(stop_args.parent_start_timing);
        let stop_timing = stop_args
            .stop_timing
            .or(self.persistent_data.stop_timing)
            .unwrap_or(stop_args.parent_stop_timing);
        ConcreteClipPlayStopTiming::resolve(start_timing, stop_timing)
    }

    fn calculate_virtual_play_pos(&self, play_args: &ClipPlayArgs) -> VirtualPosition {
        let start_timing = self
            .persistent_data
            .start_timing
            .unwrap_or(play_args.parent_start_timing);
        use ClipPlayStartTiming::*;
        match start_timing {
            Immediately => VirtualPosition::Now,
            Quantized(q) => {
                let quantized_pos =
                    QuantizedPosition::from_quantization(q, play_args.timeline, play_args.ref_pos);
                VirtualPosition::Quantized(quantized_pos)
            }
        }
    }

    pub fn stop(&mut self, args: ClipStopArgs, supplier_chain: &mut SupplierChain) {
        use ReadySubState::*;
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
                        if s.stop_request.is_none() {
                            // Not yet scheduled for stop.
                            self.state = if supplier_chain.is_playing_already(pos) {
                                // Playing
                                let resolved_stop_timing = self.resolve_stop_timing(&args);
                                use ConcreteClipPlayStopTiming::*;
                                match resolved_stop_timing {
                                    Immediately => {
                                        // Immediately. Transition to stop.
                                        Suspending(SuspendingState {
                                            next_state: StateAfterSuspension::Stopped,
                                            pos,
                                        })
                                    }
                                    Quantized(q) => {
                                        let ref_pos = args
                                            .ref_pos
                                            .unwrap_or_else(|| args.timeline.cursor_pos());
                                        let quantized_pos =
                                            args.timeline.next_quantized_pos_at(ref_pos, q);
                                        Playing(PlayingState {
                                            stop_request: Some(StopRequest::Quantized(
                                                quantized_pos,
                                            )),
                                            ..s
                                        })
                                    }
                                    UntilEndOfClip => {
                                        if self.persistent_data.looped {
                                            // Schedule
                                            supplier_chain
                                                .keep_playing_until_end_of_current_cycle(pos);
                                            Playing(PlayingState {
                                                stop_request: Some(StopRequest::AtEndOfClip),
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
                let resolved_stop_timing = self.resolve_stop_timing(&args);
                if resolved_stop_timing == ConcreteClipPlayStopTiming::Immediately {
                    // We are in another transition already. Simply change it to stop.
                    self.state = Suspending(SuspendingState {
                        next_state: StateAfterSuspension::Stopped,
                        ..s
                    });
                }
            }
        }
    }

    pub fn process(
        &mut self,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
        shared_pos: &mut SharedPos,
    ) -> (ClipPlayingOutcome, Option<RecordingState>) {
        use ReadySubState::*;
        let (outcome, changed_state, pos) = match self.state {
            Stopped | Paused(_) => return (Default::default(), None),
            Playing(s) => {
                let outcome = self.process_playing(s, args, supplier_chain);
                (outcome, None, s.pos.unwrap_or_default())
            }
            Suspending(s) => {
                let (outcome, changed_state) = self.process_suspending(s, args, supplier_chain);
                (outcome, changed_state, s.pos)
            }
        };
        shared_pos.set(pos);
        (outcome, changed_state)
    }

    fn process_playing(
        &mut self,
        s: PlayingState,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
    ) -> ClipPlayingOutcome {
        let general_info = self.prepare_playing(args, supplier_chain);
        let go = if let Some(pos) = s.pos {
            // Already counting in or playing.
            if let Some(seek_pos) = s.seek_pos {
                // Seek requested
                self.calculate_seek_go(supplier_chain, pos, seek_pos)
            } else if args.resync {
                // Resync requested
                debug!("Resync");
                self.go(s, args, supplier_chain, &general_info)
            } else {
                // Normal situation: Continue playing
                // Check if the resolve step would still arrive at the same result as our
                // frame-advancing counter.
                let compare_pos = self.resolve_virtual_pos(
                    s.virtual_pos,
                    args,
                    general_info.clip_tempo_factor,
                    supplier_chain,
                    false,
                );
                if supplier_chain.is_midi() && compare_pos != pos {
                    // This happened a lot when the MIDI_FRAME_RATE wasn't a multiple of the sample
                    // rate and PPQ.
                    debug!("ATTENTION: compare pos {} != pos {}", compare_pos, pos);
                }
                Go {
                    pos,
                    ..Go::default()
                }
            }
        } else {
            // Not counting in or playing yet.
            self.go(s, args, supplier_chain, &general_info)
        };
        // Resolve potential quantized stop position if not yet done.
        if let Some(StopRequest::Quantized(quantized_pos)) = s.stop_request {
            if !supplier_chain.stop_interaction_is_installed_already() {
                // We have a quantized stop request. Calculate distance from quantized position.
                // This should be a negative position because we should be left of the stop.
                let distance_from_quantized_stop_pos = self.calc_distance_from_quantized_pos(
                    quantized_pos,
                    args,
                    general_info.clip_tempo_factor,
                    supplier_chain,
                    true,
                );
                // Derive stop position within material.
                let stop_pos = go.pos - distance_from_quantized_stop_pos;
                let mod_stop_pos = self.modulo_frame(stop_pos, supplier_chain);
                debug!(
                    "Calculated stop position {} (mod_stop_pos = {}, go pos = {}, distance = {}, quantized pos = {:?}, tempo factor = {:?})",
                    stop_pos, mod_stop_pos, go.pos, distance_from_quantized_stop_pos, quantized_pos, general_info.clip_tempo_factor
                );
                supplier_chain.schedule_stop_interaction_at(stop_pos);
            }
        }
        let outcome = self.fill_samples(
            args,
            go.pos,
            &general_info,
            go.sample_rate_factor,
            supplier_chain,
        );
        self.state = if let Some(next_frame) = outcome.next_frame {
            // There's still something to play.
            ReadySubState::Playing(PlayingState {
                pos: Some(next_frame),
                seek_pos: go.new_seek_pos.and_then(|new_seek_pos| {
                    // Check if we reached our desired position.
                    if next_frame >= new_seek_pos as isize {
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
            // We have reached the natural or scheduled-stop (at end of clip) end. Everything that
            // needed to be played has been played in previous blocks. Audio fade outs have been
            // applied as well, so no need to go to suspending state first. Go right to stop!
            self.pre_buffer(supplier_chain, 0);
            self.reset_for_play(supplier_chain);
            ReadySubState::Stopped
        };
        outcome.clip_playing_outcome
    }

    fn go(
        &mut self,
        playing_state: PlayingState,
        args: &ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
        general_info: &SupplyRequestGeneralInfo,
    ) -> Go {
        let pos = self.resolve_virtual_pos(
            playing_state.virtual_pos,
            args,
            general_info.clip_tempo_factor,
            supplier_chain,
            true,
        );
        if supplier_chain.is_playing_already(pos) {
            debug!("Install immediate start interaction because material playing already");
            supplier_chain.install_immediate_start_interaction(pos);
        }
        Go {
            pos,
            ..Go::default()
        }
    }

    fn calculate_seek_go(
        &mut self,
        supplier_chain: &mut SupplierChain,
        pos: MaterialPos,
        seek_pos: usize,
    ) -> Go {
        // Seek requested.
        if supplier_chain.is_midi() {
            // MIDI. Let's jump to the position directly.
            Go {
                pos: seek_pos as isize,
                sample_rate_factor: 1.0,
                new_seek_pos: None,
            }
        } else {
            // Audio. Let's fast-forward if possible.
            let (sample_rate_factor, new_seek_pos) = if supplier_chain.is_playing_already(pos) {
                // Playing.
                let pos = pos as usize;
                let seek_pos = if pos < seek_pos {
                    seek_pos
                } else {
                    seek_pos + supplier_chain.section_frame_count_in_ready_state()
                };
                // We might need to fast-forward.
                let real_distance = seek_pos - pos;
                let desired_distance_in_secs = DurationInSeconds::new(0.300);
                let source_frame_rate = supplier_chain.source_frame_rate_in_ready_state();
                let desired_distance = convert_duration_in_seconds_to_frames(
                    desired_distance_in_secs,
                    source_frame_rate,
                );
                if desired_distance < real_distance {
                    // We need to fast-forward.
                    let playback_speed_factor =
                        16.0f64.min(real_distance as f64 / desired_distance as f64);
                    let sample_rate_factor = 1.0 / playback_speed_factor;
                    (sample_rate_factor, Some(seek_pos))
                } else {
                    // We are almost there anyway, so no.
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
    }

    fn resolve_virtual_pos(
        &self,
        virtual_pos: VirtualPosition,
        process_args: &ClipProcessArgs,
        clip_tempo_factor: f64,
        supplier_chain: &SupplierChain,
        log_natural_deviation: bool,
    ) -> isize {
        use VirtualPosition::*;
        match virtual_pos {
            Now => 0,
            Quantized(qp) => self.calc_distance_from_quantized_pos(
                qp,
                process_args,
                clip_tempo_factor,
                supplier_chain,
                log_natural_deviation,
            ),
        }
    }

    /// Returns the next frame to be queried.
    ///
    /// Returns `None` if end of material.
    fn fill_samples(
        &mut self,
        args: &mut ClipProcessArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        sample_rate_factor: f64,
        supplier_chain: &mut SupplierChain,
    ) -> FillSamplesOutcome {
        supplier_chain.prepare_supply();
        let dest_sample_rate = Hz::new(args.dest_sample_rate.get() * sample_rate_factor);
        let is_midi = supplier_chain.is_midi();
        let response = if is_midi {
            self.fill_samples_midi(args, start_frame, info, dest_sample_rate, supplier_chain)
        } else {
            self.fill_samples_audio(args, start_frame, info, dest_sample_rate, supplier_chain)
        };
        let (num_frames_written, next_frame) = match response.status {
            SupplyResponseStatus::PleaseContinue => (
                args.dest_buffer.frame_count(),
                Some(start_frame + response.num_frames_consumed as isize),
            ),
            SupplyResponseStatus::ReachedEnd { num_frames_written } => (num_frames_written, None),
        };
        FillSamplesOutcome {
            clip_playing_outcome: ClipPlayingOutcome {
                num_audio_frames_written: if is_midi { 0 } else { num_frames_written },
            },
            next_frame,
        }
    }

    fn fill_samples_audio(
        &mut self,
        args: &mut ClipProcessArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        dest_sample_rate: Hz,
        supplier_chain: &mut SupplierChain,
    ) -> SupplyResponse {
        let request = SupplyAudioRequest {
            start_frame,
            dest_sample_rate: Some(dest_sample_rate),
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-audio",
                note: "",
                is_realtime: true,
            },
            parent_request: None,
            general_info: info,
        };
        supplier_chain.supply_audio(&request, args.dest_buffer)
    }

    fn fill_samples_midi(
        &mut self,
        args: &mut ClipProcessArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        dest_sample_rate: Hz,
        supplier_chain: &mut SupplierChain,
    ) -> SupplyResponse {
        let request = SupplyMidiRequest {
            start_frame,
            dest_frame_count: args.dest_buffer.frame_count(),
            dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: 0,
                requester: "root-midi",
                note: "",
                is_realtime: true,
            },
            parent_request: None,
            general_info: info,
        };
        supplier_chain.supply_midi(&request, args.midi_event_list)
    }

    fn prepare_playing(
        &mut self,
        args: &ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
    ) -> SupplyRequestGeneralInfo {
        let tempo_factor = self.calc_tempo_factor(args.timeline_tempo, supplier_chain.is_midi());
        let general_info = SupplyRequestGeneralInfo {
            audio_block_timeline_cursor_pos: args.timeline_cursor_pos,
            audio_block_length: args.dest_buffer.frame_count(),
            output_frame_rate: args.dest_sample_rate,
            timeline_tempo: args.timeline_tempo,
            clip_tempo_factor: tempo_factor,
        };
        supplier_chain.set_tempo_factor(tempo_factor);
        general_info
    }

    fn calc_tempo_factor(&self, timeline_tempo: Bpm, is_midi: bool) -> f64 {
        if let Some(clip_tempo) = self.tempo(is_midi) {
            calc_tempo_factor(clip_tempo, timeline_tempo)
        } else {
            1.0
        }
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
    /// start of the bar. But first, we need the relative count-in anyway for pickup beats,
    /// which start to play during count-in time. And second, just counting is cheaper
    /// than repeatedly doing time/beat mapping.
    ///
    /// 2. We resolve the count-in length here, not at the time the play is requested.
    /// Reason: Here we have block information such as block length and frame rate available.
    /// That's not an urgent reason ... we could always cache this information and thus make it
    /// available in the play request itself. Or we make sure that play/stop is always triggered
    /// via receiving in get_samples()! That's good! TODO-medium Implement it.
    /// In the past there were more urgent reasons but they are gone. I'll document them here
    /// because they might remove doubt in case of possible future refactorings:
    ///
    /// 2a) The play request didn't happen in a real-time thread but in the main thread.
    /// At that time it was important to resolve in get_samples() because the start time of the
    /// next bar at play-request time was not necessarily the same as the one in the get_samples()
    /// call, which would lead to wrong results. However, today, play requests always happen in
    /// the real-time thread (a change introduced in favor of a lock-free design).
    ///
    /// 2b) I still thought that it would be better to do it here in case "Live FX multiprocessing"
    /// is enabled. If this is enabled, it means get_samples() will in most situations be called in
    /// a different real-time thread (some REAPER worker thread) than the play-request code
    /// (audio interface thread). I worried that GetPlayPosition2Ex() in the worker thread would
    /// return a different position as the audio interface thread would do. However, Justin
    /// assured that the worker threads are designed to be synchronous with the audio interface
    /// thread and they return the same values. So this is not a reason anymore.
    fn calc_distance_from_quantized_pos(
        &self,
        quantized_pos: QuantizedPosition,
        args: &ClipProcessArgs,
        clip_tempo_factor: f64,
        supplier_chain: &SupplierChain,
        log_natural_deviation: bool,
    ) -> isize {
        // Basics
        let block_length_in_timeline_frames = args.dest_buffer.frame_count();
        let source_frame_rate = supplier_chain.source_frame_rate_in_ready_state();
        let timeline_frame_rate = args.dest_sample_rate;
        // Essential calculation
        let quantized_timeline_pos = args.timeline.pos_of_quantized_pos(quantized_pos);
        let rel_pos_from_quant_in_secs = args.timeline_cursor_pos - quantized_timeline_pos;
        let rel_pos_from_quant_in_source_frames = convert_position_in_seconds_to_frames(
            rel_pos_from_quant_in_secs,
            supplier_chain.source_frame_rate_in_ready_state(),
        );
        if log_natural_deviation && quantized_pos.denominator() == 1 {
            // Quantization to bar
            if let Some(clip_tempo) = self.tempo(supplier_chain.is_midi()) {
                // Plus, we react to tempo changes.
                let args = LogNaturalDeviationArgs {
                    start_bar: quantized_pos.position() as _,
                    block_length: args.dest_buffer.frame_count(),
                    timeline: &args.timeline,
                    timeline_cursor_pos: args.timeline_cursor_pos,
                    clip_tempo_factor,
                    timeline_frame_rate,
                    source_frame_rate,
                    start_bar_timeline_pos: quantized_timeline_pos,
                    clip_tempo,
                };
                self.log_natural_deviation(args, supplier_chain);
            }
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
        // 2. If the downbeat is > 0, the count-in phase plays source material already.
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
            rel_pos_from_quant_in_source_frames,
            clip_tempo_factor,
            block_length_in_source_frames,
        )
    }

    fn process_suspending(
        &mut self,
        s: SuspendingState,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
    ) -> (ClipPlayingOutcome, Option<RecordingState>) {
        let general_info = self.prepare_playing(args, supplier_chain);
        // TODO-medium We could do that already when changing to suspended. That saves us the
        //  check if a stop interaction is installed already.
        if !supplier_chain.stop_interaction_is_installed_already() {
            supplier_chain.install_immediate_stop_interaction(s.pos);
        }
        let outcome = self.fill_samples(args, s.pos, &general_info, 1.0, supplier_chain);
        self.state = if let Some(next_frame) = outcome.next_frame {
            // Suspension not finished yet.
            ReadySubState::Suspending(SuspendingState {
                pos: next_frame,
                ..s
            })
        } else {
            // Suspension finished.
            use StateAfterSuspension::*;
            self.reset_for_play(supplier_chain);
            match s.next_state {
                Playing(s) => ReadySubState::Playing(s),
                Paused => ReadySubState::Paused(PausedState { pos: s.pos }),
                Stopped => {
                    self.pre_buffer(supplier_chain, 0);
                    ReadySubState::Stopped
                }
                Recording(s) => return (outcome.clip_playing_outcome, Some(s)),
            }
        };
        (outcome.clip_playing_outcome, None)
    }

    fn pre_buffer(&mut self, supplier_chain: &mut SupplierChain, next_expected_pos: isize) {
        if supplier_chain.is_midi() {
            return;
        }
        let req = PreBufferFillRequest {
            start_frame: next_expected_pos,
        };
        supplier_chain.pre_buffer(req);
    }

    fn reset_for_play(&mut self, supplier_chain: &mut SupplierChain) {
        supplier_chain.reset_for_play(self.persistent_data.looped);
    }

    fn log_natural_deviation(
        &self,
        args: LogNaturalDeviationArgs<impl Timeline>,
        supplier_chain: &SupplierChain,
    ) {
        // Assuming a constant tempo and time signature during one cycle
        let clip_duration = supplier_chain.section_duration_in_ready_state();
        let beat_count = calculate_beat_count(args.clip_tempo, clip_duration);
        let bar_count = (beat_count as f64 / 4.0).ceil() as u32;
        let end_bar = args.start_bar + bar_count as i32;
        let end_bar_timeline_pos = args.timeline.pos_of_bar(end_bar);
        debug_assert!(
            end_bar_timeline_pos > args.start_bar_timeline_pos,
            "end_bar_timeline_pos {} <= start_bar_timeline_pos {}",
            end_bar_timeline_pos,
            args.start_bar_timeline_pos
        );
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
        let source_cycle_length_in_secs = supplier_chain.section_duration_in_ready_state();
        let source_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
            source_cycle_length_in_secs,
            args.timeline_frame_rate,
        );
        let source_cycle_length_in_source_frames =
            supplier_chain.section_frame_count_in_ready_state();
        // Block length
        let block_length_in_timeline_frames = args.block_length;
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
        debug!(
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

    pub fn midi_overdub(&mut self) {
        use ReadySubState::*;
        // TODO-medium Maybe we should start to play if not yet playing
        if let Playing(s) = self.state {
            self.state = Playing(PlayingState {
                overdubbing: true,
                ..s
            });
        }
    }

    pub fn record(
        &mut self,
        args: ClipRecordArgs,
        project: Option<Project>,
        supplier_chain: &mut SupplierChain,
    ) -> Option<RecordingState> {
        let timeline = clip_timeline(project, false);
        let trigger_timeline_pos = timeline.cursor_pos();
        let tempo = timeline.tempo_at(trigger_timeline_pos);
        supplier_chain.prepare_recording(
            args.input,
            project,
            trigger_timeline_pos,
            tempo,
            args.detect_downbeat,
            args.timing,
        );
        let recording_state = RecordingState {
            trigger_timeline_pos,
            looped: args.looped,
            timing: args.timing,
            input: args.input,
            rollback_data: {
                let data = RollbackData {
                    persistent_data: self.persistent_data,
                };
                Some(data)
            },
        };
        use ReadySubState::*;
        match self.state {
            Stopped => Some(recording_state),
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if supplier_chain.is_playing_already(pos) {
                        self.state = Suspending(SuspendingState {
                            next_state: StateAfterSuspension::Recording(recording_state),
                            pos,
                        });
                        None
                    } else {
                        Some(recording_state)
                    }
                } else {
                    Some(recording_state)
                }
            }
            Suspending(s) => {
                self.state = Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Recording(recording_state),
                    ..s
                });
                None
            }
            Paused(_) => Some(recording_state),
        }
    }
    pub fn pause(&mut self, supplier_chain: &SupplierChain) {
        use ReadySubState::*;
        match self.state {
            Stopped | Paused(_) => {}
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if supplier_chain.is_playing_already(pos) {
                        // Playing. Pause!
                        self.state = Suspending(SuspendingState {
                            next_state: StateAfterSuspension::Paused,
                            pos,
                        });
                    }
                }
                // If not yet playing, we don't do anything at the moment.
                // TODO-medium In future, we could defer the clip scheduling to the future. I think
                //  that would feel natural.
            }
            Suspending(s) => {
                self.state = Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Paused,
                    ..s
                });
            }
        }
    }

    pub fn seek(&mut self, desired_pos: UnitValue, supplier_chain: &SupplierChain) {
        let frame_count = supplier_chain.section_frame_count_in_ready_state();
        let desired_frame = adjust_proportionally_positive(frame_count as f64, desired_pos.get());
        use ReadySubState::*;
        match self.state {
            Stopped | Suspending(_) => {}
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if supplier_chain.is_playing_already(pos) {
                        let up_cycled_frame =
                            self.up_cycle_frame(desired_frame, pos, frame_count, supplier_chain);
                        self.state = Playing(PlayingState {
                            seek_pos: Some(up_cycled_frame),
                            ..s
                        });
                    }
                }
            }
            Paused(s) => {
                let up_cycled_frame =
                    self.up_cycle_frame(desired_frame, s.pos, frame_count, supplier_chain);
                self.state = Paused(PausedState {
                    pos: up_cycled_frame as isize,
                });
            }
        }
    }

    fn up_cycle_frame(
        &self,
        frame: usize,
        offset_pos: isize,
        frame_count: usize,
        supplier_chain: &SupplierChain,
    ) -> usize {
        let current_cycle = supplier_chain.get_cycle_at_frame(offset_pos);
        current_cycle * frame_count + frame
    }

    pub fn play_state(&self) -> ClipPlayState {
        use ReadySubState::*;
        match self.state {
            Stopped => ClipPlayState::Stopped,
            Playing(s) => {
                if s.overdubbing {
                    ClipPlayState::Recording
                } else if s.stop_request.is_some() {
                    ClipPlayState::ScheduledForStop
                } else if let Some(pos) = s.pos {
                    // It's correct that we don't consider the downbeat here. We want to expose
                    // the count-in phase as count-in phase, even some pickup beats are playing
                    // already.
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
                StateAfterSuspension::Paused => ClipPlayState::Paused,
                StateAfterSuspension::Stopped => ClipPlayState::Stopped,
                StateAfterSuspension::Recording(_) => ClipPlayState::Recording,
            },
            Paused(_) => ClipPlayState::Paused,
        }
    }

    fn schedule_play_internal(&mut self, virtual_pos: VirtualPosition) {
        self.state = ReadySubState::Playing(PlayingState {
            virtual_pos,
            ..Default::default()
        });
    }
}

impl RecordingState {
    pub fn set_looped(&mut self, looped: bool) {
        self.looped = looped;
    }

    pub fn stop(
        &mut self,
        args: ClipStopArgs,
        supplier_chain: &mut SupplierChain,
    ) -> RecordingStopOutcome {
        use RecordTiming::*;
        use RecordingStopOutcome::*;
        match self.timing {
            Unsynced => {
                let ready_state =
                    self.finish_recording(self.looped, None, supplier_chain, &args.timeline);
                TransitionToReady(ready_state)
            }
            Synced { start_bar, end_bar } => {
                let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
                let next_bar = args.timeline.next_bar_at(ref_pos);
                if next_bar <= start_bar {
                    // Zero point of recording hasn't even been reached yet. Try to roll back.
                    if let Some(rollback_data) = &self.rollback_data {
                        // We have a previous source that we can roll back to.
                        supplier_chain.rollback_recording().unwrap();
                        let ready_state = ReadyState {
                            state: ReadySubState::Stopped,
                            persistent_data: rollback_data.persistent_data,
                        };
                        TransitionToReady(ready_state)
                    } else {
                        // There was nothing to roll back to. How sad.
                        ClearSlot
                    }
                } else {
                    // We are recording already.
                    if end_bar.is_some() {
                        // End already scheduled. Take care of stopping after recording.
                        self.looped = false;
                    } else {
                        // End not scheduled yet. Schedule end.
                        supplier_chain.schedule_end_of_recording(next_bar, &args.timeline);
                        self.timing = Synced {
                            start_bar,
                            end_bar: Some(next_bar),
                        };
                    }
                    KeepState
                }
            }
        }
    }

    fn process(
        &mut self,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
    ) -> Option<ReadyState> {
        if let RecordTiming::Synced {
            start_bar,
            end_bar: Some(end_bar),
        } = self.timing
        {
            if args.timeline.next_bar_at(args.timeline_cursor_pos) >= end_bar {
                // Close to scheduled recording end.
                let block_length_in_timeline_frames = args.dest_buffer.frame_count();
                let timeline_frame_rate = args.dest_sample_rate;
                let block_length_in_secs = convert_duration_in_frames_to_seconds(
                    block_length_in_timeline_frames,
                    timeline_frame_rate,
                );
                let block_end_pos = args.timeline_cursor_pos + block_length_in_secs;
                let downbeat_pos = supplier_chain.downbeat_pos_during_recording(&args.timeline);
                let record_end_pos = args.timeline.pos_of_bar(end_bar) - downbeat_pos;
                if block_end_pos >= record_end_pos {
                    // We have recorded the last block.
                    let ready_state = self.finish_recording(
                        self.looped,
                        Some((start_bar, end_bar)),
                        supplier_chain,
                        &args.timeline,
                    );
                    Some(ready_state)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn finish_recording(
        self,
        play_after: bool,
        start_and_end_bar: Option<(i32, i32)>,
        supplier_chain: &mut SupplierChain,
        timeline: &dyn Timeline,
    ) -> ReadyState {
        let outcome = supplier_chain.commit_recording(timeline).unwrap();
        // Calculate section boundaries
        // Set section boundaries for perfect timing.
        supplier_chain.set_section_bounds(outcome.section_start_frame, outcome.section_frame_count);
        // Set downbeat.
        supplier_chain.set_downbeat_in_frames(outcome.normalized_downbeat_frame);
        // Change state
        ReadyState {
            state: if play_after {
                ReadySubState::Playing(PlayingState {
                    virtual_pos: match start_and_end_bar {
                        None => VirtualPosition::Now,
                        Some((_, end_bar)) => VirtualPosition::Quantized(
                            QuantizedPosition::new(end_bar as _, 1).unwrap(),
                        ),
                    },
                    ..Default::default()
                })
            } else {
                ReadySubState::Stopped
            },
            persistent_data: PersistentPlayData {
                // TODO-high-record Set start timing
                start_timing: None,
                // TODO-high-record Set stop timing
                stop_timing: None,
                looped: play_after,
                // TODO-high-record Set time base
                time_base: ClipTimeBase::Time,
            },
        }
    }
}

#[derive(PartialEq)]
pub enum SlotInstruction {
    KeepSlot,
    ClearSlot,
}

enum RecordingStopOutcome {
    KeepState,
    TransitionToReady(ReadyState),
    ClearSlot,
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

#[derive(Clone, Debug)]
pub struct ClipPlayArgs<'a> {
    pub parent_start_timing: ClipPlayStartTiming,
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
}

#[derive(Debug)]
pub struct ClipStopArgs<'a> {
    pub parent_start_timing: ClipPlayStartTiming,
    pub parent_stop_timing: ClipPlayStopTiming,
    pub stop_timing: Option<ClipPlayStopTiming>,
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
}

#[derive(Copy, Clone, Debug)]
pub enum VirtualPosition {
    Now,
    Quantized(QuantizedPosition),
}

impl Default for VirtualPosition {
    fn default() -> Self {
        Self::Now
    }
}

impl VirtualPosition {
    pub fn is_quantized(&self) -> bool {
        matches!(self, VirtualPosition::Quantized(_))
    }
}

pub struct ClipRecordArgs {
    pub looped: bool,
    pub input: ClipRecordInput,
    pub timing: RecordTiming,
    pub detect_downbeat: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum ClipRecordInput {
    Midi,
    Audio,
}

impl ClipRecordInput {
    pub fn is_midi(&self) -> bool {
        matches!(self, Self::Midi)
    }
}

#[derive(PartialEq, Debug)]
pub enum ClipStopBehavior {
    Immediately,
    EndOfClip,
}

pub struct ClipProcessArgs<'a, 'b> {
    /// The destination buffer dictates the desired output frame count but it doesn't dictate the
    /// channel count! Its channel count should always match the channel count of the clip itself.
    pub dest_buffer: &'a mut AudioBufMut<'b>,
    pub dest_sample_rate: Hz,
    pub midi_event_list: &'a mut BorrowedMidiEventList,
    pub timeline: &'a HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub timeline_tempo: Bpm,
    /// Tells the clip to re-calculate its ideal play position (set when doing resume-from-pause).
    pub resync: bool,
}

struct LogNaturalDeviationArgs<T: Timeline> {
    start_bar: i32,
    block_length: usize,
    timeline: T,
    timeline_cursor_pos: PositionInSeconds,
    // timeline_tempo: Bpm,
    clip_tempo_factor: f64,
    timeline_frame_rate: Hz,
    source_frame_rate: Hz,
    start_bar_timeline_pos: PositionInSeconds,
    clip_tempo: Bpm,
}

const MIN_TEMPO_FACTOR: f64 = 0.0000000001;

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
            Stopped => UnitValue::new(0.1),
            ScheduledForPlay => UnitValue::new(0.75),
            Playing => UnitValue::MAX,
            Paused => UnitValue::new(0.5),
            ScheduledForStop => UnitValue::new(0.25),
            Recording => UnitValue::new(0.60),
        }
    }

    pub fn is_advancing(&self) -> bool {
        use ClipPlayState::*;
        matches!(self, ScheduledForPlay | Playing | ScheduledForStop)
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
    ClipVolume(Db),
    ClipLooped(bool),
    ClipPosition(UnitValue),
}

#[derive(Debug)]
pub struct QualifiedClipChangedEvent {
    pub slot_coordinates: ClipSlotCoordinates,
    pub event: ClipChangedEvent,
}

pub struct PlayOutcome {
    pub virtual_pos: VirtualPosition,
}

#[derive(PartialEq)]
enum ConcreteClipPlayStopTiming {
    Immediately,
    Quantized(EvenQuantization),
    UntilEndOfClip,
}

impl ConcreteClipPlayStopTiming {
    pub fn resolve(start_timing: ClipPlayStartTiming, stop_timing: ClipPlayStopTiming) -> Self {
        use ClipPlayStopTiming::*;
        match stop_timing {
            LikeClipStartTiming => match start_timing {
                ClipPlayStartTiming::Immediately => Self::Immediately,
                ClipPlayStartTiming::Quantized(q) => Self::Quantized(q),
            },
            Immediately => Self::Immediately,
            Quantized(q) => Self::Quantized(q),
            UntilEndOfClip => Self::UntilEndOfClip,
        }
    }
}

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

/// Returns `None` if time base is not "Beat".
pub fn determine_tempo_from_time_base(time_base: &ClipTimeBase, is_midi: bool) -> Option<Bpm> {
    use ClipTimeBase::*;
    match time_base {
        Time => None,
        Beat(b) => Some(determine_tempo_from_beat_time_base(b, is_midi)),
    }
}

pub fn calc_tempo_factor(clip_tempo: Bpm, timeline_tempo: Bpm) -> f64 {
    let timeline_tempo_factor = timeline_tempo.get() / clip_tempo.get();
    timeline_tempo_factor.max(MIN_TEMPO_FACTOR)
}

fn determine_tempo_from_beat_time_base(beat_time_base: &BeatTimeBase, is_midi: bool) -> Bpm {
    if is_midi {
        Bpm::new(MIDI_BASE_BPM)
    } else {
        let tempo = beat_time_base
            .audio_tempo
            .expect("material has time base 'beat' but no tempo");
        Bpm::new(tempo.get())
    }
}

#[derive(Default)]
pub struct ClipPlayingOutcome {
    pub num_audio_frames_written: usize,
}

struct FillSamplesOutcome {
    clip_playing_outcome: ClipPlayingOutcome,
    next_frame: Option<isize>,
}
