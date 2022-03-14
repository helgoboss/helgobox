use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_other_frame_rate,
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    convert_position_in_seconds_to_frames,
};
use crate::main::{create_pcm_source_from_api_source, ClipSlotCoordinates};
use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, ChainEquipment, ChainSettings, CompleteRecordingData,
    KindSpecificRecordingOutcome, MaterialInfo, MidiSupplier, RecordTiming, Recorder,
    RecorderRequest, RecordingArgs, RecordingEquipment, SupplierChain, SupplyAudioRequest,
    SupplyMidiRequest, SupplyRequestGeneralInfo, SupplyRequestInfo, SupplyResponse,
    SupplyResponseStatus, WithMaterialInfo, WriteAudioRequest, WriteMidiRequest, MIDI_BASE_BPM,
    MIDI_FRAME_RATE,
};
use crate::rt::tempo_util::determine_tempo_from_time_base;
use crate::rt::{ColumnSettings, OverridableMatrixSettings};
use crate::timeline::{clip_timeline, HybridTimeline, Timeline};
use crate::{ClipEngineResult, ErrorWithPayload, QuantizedPosition};
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use playtime_api as api;
use playtime_api::{
    ClipAudioSettings, ClipMidiSettings, ClipPlayStartTiming, ClipPlayStopTiming, ClipTimeBase, Db,
    EvenQuantization, MatrixClipRecordSettings, PositiveSecond,
};
use reaper_high::Project;
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, OwnedPcmSource, PcmSourceTransfer,
    PositionInSeconds,
};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub struct Clip {
    supplier_chain: SupplierChain,
    state: ClipState,
    project: Option<Project>,
    shared_pos: SharedPos,
}

/// Contains only the state that's relevant for playing *and* not kept or not kept sufficiently in
/// the supply chain. E.g. the time stretch mode is not kept here because the time stretcher
/// supplier holds it itself.
#[derive(Copy, Clone, Debug)]
struct PlaySettings {
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
    play_settings: PlaySettings,
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
    /// Marks the current record position
    ///
    /// - Can be negative for count-in.
    /// - Is advanced on each audio block respecting, faster or slower depending on the tempo.
    /// - I guess only necessary for recording MIDI because we can write MIDI anywhere into the
    ///   source so we need to know where exactly.
    pub pos: MaterialPos,
    // TODO-high Make clear what's the difference of RecordTiming to the timing stored in the recorder.
    timing: RecordTiming,
    rollback_data: Option<RollbackData>,
    settings: MatrixClipRecordSettings,
    initial_play_start_timing: ClipPlayStartTiming,
}

#[derive(Copy, Clone, Debug)]
struct RollbackData {
    play_settings: PlaySettings,
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

impl Clip {
    /// Must not call in real-time thread!
    pub fn ready(
        api_source: &api::Source,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
        clip_settings: &ProcessingRelevantClipSettings,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
    ) -> ClipEngineResult<Self> {
        let pcm_source = create_pcm_source_from_api_source(api_source, permanent_project)?;
        let ready_state = ReadyState {
            state: ReadySubState::Stopped,
            play_settings: clip_settings.create_play_settings(),
        };
        let mut supplier_chain = SupplierChain::new(
            Recorder::ready(pcm_source, recorder_request_sender.clone()),
            chain_equipment.clone(),
        )?;
        let chain_settings = clip_settings.create_chain_settings(matrix_settings, column_settings);
        supplier_chain.configure_complete_chain(chain_settings)?;
        supplier_chain.pre_buffer_simple(0);
        let clip = Self {
            supplier_chain,
            state: ClipState::Ready(ready_state),
            project: permanent_project,
            shared_pos: Default::default(),
        };
        Ok(clip)
    }

    pub fn recording(
        instruction: RecordNewClipInstruction,
        audio_request_props: BasicAudioRequestProps,
    ) -> Self {
        let tempo = instruction
            .timeline
            .tempo_at(instruction.timeline_cursor_pos);
        let initial_pos = resolve_initial_recording_pos(
            instruction.timing,
            &instruction.timeline,
            instruction.timeline_cursor_pos,
            tempo,
            audio_request_props,
            instruction.is_midi,
        );
        let recording_state = RecordingState {
            pos: initial_pos,
            rollback_data: None,
            timing: instruction.timing,
            settings: instruction.settings,
            initial_play_start_timing: instruction.initial_play_start_timing,
        };
        Self {
            supplier_chain: instruction.supplier_chain,
            state: ClipState::Recording(recording_state),
            project: instruction.project,
            shared_pos: instruction.shared_pos,
        }
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
    pub fn stop<H: HandleStopEvent>(
        &mut self,
        args: ClipStopArgs,
        event_handler: &H,
    ) -> StopSlotInstruction {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                s.stop(args, &mut self.supplier_chain, event_handler);
                StopSlotInstruction::KeepSlot
            }
            Recording(s) => {
                use ClipRecordingStopOutcome::*;
                match s.stop(args, &mut self.supplier_chain, event_handler) {
                    KeepState => StopSlotInstruction::KeepSlot,
                    TransitionToReady(ready_state) => {
                        self.state = Ready(ready_state);
                        StopSlotInstruction::KeepSlot
                    }
                    ClearSlot => StopSlotInstruction::ClearSlot,
                }
            }
        }
    }

    pub fn set_looped(&mut self, looped: bool) -> ClipEngineResult<()> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                s.set_looped(looped, &mut self.supplier_chain);
                Ok(())
            }
            Recording(_) => Err("can't set looped while recording"),
        }
    }

    pub fn looped(&self) -> bool {
        use ClipState::*;
        match self.state {
            Ready(s) => s.play_settings.looped,
            Recording(_) => false,
        }
    }

    pub fn midi_overdub(
        &mut self,
        args: MidiOverdubInstruction,
    ) -> Result<(), ErrorWithPayload<MidiOverdubInstruction>> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.midi_overdub(args, &mut self.supplier_chain),
            Recording(_) => Err(ErrorWithPayload::new("clip is recording", args)),
        }
    }

    pub fn record(
        &mut self,
        args: ClipRecordArgs,
        audio_request_props: BasicAudioRequestProps,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
    ) -> Result<(), ErrorWithPayload<ClipRecordArgs>> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                let new_state = s.record(
                    args,
                    self.project,
                    &mut self.supplier_chain,
                    audio_request_props,
                    matrix_settings,
                    column_settings,
                );
                if let Some(recording_state) = new_state {
                    self.state = Recording(recording_state);
                }
                Ok(())
            }
            Recording(_) => Err(ErrorWithPayload::new("already recording", args)),
        }
    }

    pub fn pause(&mut self) {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.pause(&self.supplier_chain),
            Recording(_) => {}
        }
    }

    pub fn seek(&mut self, desired_pos: UnitValue) -> ClipEngineResult<()> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.seek(desired_pos, &self.supplier_chain),
            Recording(_) => Err("recording"),
        }
    }

    pub fn write_midi(&mut self, request: WriteMidiRequest) {
        use ClipState::*;
        let source_frame = match &self.state {
            Ready(s) => match s.state {
                ReadySubState::Playing(PlayingState {
                    overdubbing: true,
                    pos: Some(pos),
                    ..
                }) => {
                    let material_info = match self.supplier_chain.material_info() {
                        Ok(i) => i,
                        Err(_) => return,
                    };
                    modulo_frame(pos, material_info.frame_count())
                }
                _ => return,
            },
            Recording(s) => s.pos,
        };
        if source_frame < 0 {
            return;
        }
        let source_seconds =
            convert_duration_in_frames_to_seconds(source_frame as usize, MIDI_FRAME_RATE);
        self.supplier_chain.write_midi(request, source_seconds);
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

    pub fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier_chain.material_info()
    }

    pub fn play_state(&self) -> ClipPlayState {
        use ClipState::*;
        match &self.state {
            Ready(s) => s.play_state(),
            Recording(_) => ClipPlayState::Recording,
        }
    }

    pub fn process<H: HandleStopEvent>(
        &mut self,
        args: &mut ClipProcessArgs,
        event_handler: &H,
    ) -> ClipPlayingOutcome {
        use ClipState::*;
        let (outcome, changed_state) = match &mut self.state {
            Ready(s) => {
                let (outcome, changed_state) =
                    s.process(args, &mut self.supplier_chain, &mut self.shared_pos);
                (Some(outcome), changed_state.map(Recording))
            }
            Recording(s) => {
                let changed_state = s.process(args, &mut self.supplier_chain, event_handler);
                (None, changed_state.map(Ready))
            }
        };
        let outcome = if let Some(s) = changed_state {
            self.state = s;
            if s.is_playing() {
                // Changed from record to playing. Don't miss any samples!
                Some(self.process(args, event_handler))
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
    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.play_settings.time_base, is_midi)
    }

    pub fn set_looped(&mut self, looped: bool, supplier_chain: &mut SupplierChain) {
        self.play_settings.looped = looped;
        if !looped {
            if let ReadySubState::Playing(PlayingState { pos: Some(pos), .. }) = self.state {
                supplier_chain.keep_playing_until_end_of_current_cycle(pos);
                return;
            }
        }
        supplier_chain.set_looped(self.play_settings.looped);
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
                            // TODO-high We should pre-buffer here but currently we can't do it
                            //  accurately because the position is not ready yet. However, as soon
                            //  as we resolve the position already here, let's use that one!
                            supplier_chain.pre_buffer_simple(0);
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
        let start_timing = stop_args.resolve_start_timing(self.play_settings.start_timing);
        let stop_timing = stop_args.resolve_stop_timing(self.play_settings.stop_timing);
        ConcreteClipPlayStopTiming::resolve(start_timing, stop_timing)
    }

    fn calculate_virtual_play_pos(&self, play_args: &ClipPlayArgs) -> VirtualPosition {
        let start_timing = play_args.resolve_start_timing(self.play_settings.start_timing);
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

    /// Stops the clip.
    ///
    /// By default, if it's overdubbing, it just stops the overdubbing (a second call will make
    /// it stop playing).
    pub fn stop<H: HandleStopEvent>(
        &mut self,
        args: ClipStopArgs,
        supplier_chain: &mut SupplierChain,
        event_handler: &H,
    ) {
        use ReadySubState::*;
        match self.state {
            Stopped => {}
            Playing(s) => {
                if s.overdubbing {
                    // Currently recording overdub. Stop recording.
                    self.state = Playing(PlayingState {
                        overdubbing: false,
                        ..s
                    });
                    if let Some(mirror_source) = supplier_chain.take_midi_overdub_mirror_source() {
                        event_handler.midi_overdub_finished(mirror_source);
                    }
                    if !args.enforce_play_stop {
                        // Continue playing
                        return;
                    }
                }
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
                                    let ref_pos =
                                        args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
                                    let quantized_pos =
                                        args.timeline.next_quantized_pos_at(ref_pos, q);
                                    Playing(PlayingState {
                                        stop_request: Some(StopRequest::Quantized(quantized_pos)),
                                        ..s
                                    })
                                }
                                UntilEndOfClip => {
                                    if self.play_settings.looped {
                                        // Schedule
                                        supplier_chain.keep_playing_until_end_of_current_cycle(pos);
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
        let material_info = match supplier_chain.material_info() {
            Ok(i) => i,
            Err(_) => return Default::default(),
        };
        let general_info = self.prepare_playing(args, supplier_chain, material_info.is_midi());
        let go = if let Some(pos) = s.pos {
            // Already counting in or playing.
            if let Some(seek_pos) = s.seek_pos {
                // Seek requested
                self.calculate_seek_go(supplier_chain, pos, seek_pos, &material_info)
            } else if args.resync {
                // Resync requested
                debug!("Resync");
                let go = self.go(
                    s,
                    args,
                    supplier_chain,
                    general_info.clip_tempo_factor,
                    &material_info,
                );
                supplier_chain.pre_buffer_simple(go.pos);
                go
            } else {
                // Normal situation: Continue playing
                // Check if the resolve step would still arrive at the same result as our
                // frame-advancing counter.
                let compare_pos = resolve_virtual_pos(
                    s.virtual_pos,
                    args,
                    general_info.clip_tempo_factor,
                    false,
                    &material_info,
                    None,
                );
                if material_info.is_midi() && compare_pos != pos {
                    // This happened a lot when the MIDI_FRAME_RATE wasn't a multiple of the sample
                    // rate and PPQ.
                    // debug!("ATTENTION: compare pos {} != pos {}", compare_pos, pos);
                }
                Go {
                    pos,
                    ..Go::default()
                }
            }
        } else {
            // Not counting in or playing yet.
            self.go(
                s,
                args,
                supplier_chain,
                general_info.clip_tempo_factor,
                &material_info,
            )
        };
        // Resolve potential quantized stop position if not yet done.
        if let Some(StopRequest::Quantized(quantized_pos)) = s.stop_request {
            if !supplier_chain.stop_interaction_is_installed_already() {
                // We have a quantized stop request. Calculate distance from quantized position.
                // This should be a negative position because we should be left of the stop.
                let distance_from_quantized_stop_pos = resolve_virtual_pos(
                    VirtualPosition::Quantized(quantized_pos),
                    args,
                    general_info.clip_tempo_factor,
                    false,
                    &material_info,
                    None,
                );
                // Derive stop position within material.
                let stop_pos = go.pos - distance_from_quantized_stop_pos;
                let mod_stop_pos = modulo_frame(stop_pos, material_info.frame_count());
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
            &material_info,
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
            supplier_chain.pre_buffer_simple(0);
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
        clip_tempo_factor: f64,
        material_info: &MaterialInfo,
    ) -> Go {
        let tempo = self.tempo(material_info.is_midi());
        let pos = resolve_virtual_pos(
            playing_state.virtual_pos,
            args,
            clip_tempo_factor,
            true,
            material_info,
            tempo,
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
        material_info: &MaterialInfo,
    ) -> Go {
        // Seek requested.
        if material_info.is_midi() {
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
                    seek_pos + material_info.frame_count()
                };
                // We might need to fast-forward.
                let real_distance = seek_pos - pos;
                let desired_distance_in_secs = DurationInSeconds::new(0.300);
                let source_frame_rate = material_info.frame_rate();
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
        material_info: &MaterialInfo,
    ) -> FillSamplesOutcome {
        let dest_sample_rate = Hz::new(args.dest_sample_rate.get() * sample_rate_factor);
        let is_midi = material_info.is_midi();
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
        is_midi: bool,
    ) -> SupplyRequestGeneralInfo {
        let tempo_factor = self.calc_tempo_factor(args.timeline_tempo, is_midi);
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

    fn process_suspending(
        &mut self,
        s: SuspendingState,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
    ) -> (ClipPlayingOutcome, Option<RecordingState>) {
        let material_info = match supplier_chain.material_info() {
            Ok(i) => i,
            Err(_) => return (Default::default(), None),
        };
        let general_info = self.prepare_playing(args, supplier_chain, material_info.is_midi());
        // TODO-medium We could do that already when changing to suspended. That saves us the
        //  check if a stop interaction is installed already.
        if !supplier_chain.stop_interaction_is_installed_already() {
            supplier_chain.install_immediate_stop_interaction(s.pos);
        }
        let outcome = self.fill_samples(
            args,
            s.pos,
            &general_info,
            1.0,
            supplier_chain,
            &material_info,
        );
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
                    supplier_chain.pre_buffer_simple(0);
                    ReadySubState::Stopped
                }
                Recording(s) => return (outcome.clip_playing_outcome, Some(s)),
            }
        };
        (outcome.clip_playing_outcome, None)
    }

    fn reset_for_play(&mut self, supplier_chain: &mut SupplierChain) {
        supplier_chain.reset_for_play(self.play_settings.looped);
    }

    pub fn midi_overdub(
        &mut self,
        args: MidiOverdubInstruction,
        supplier_chain: &mut SupplierChain,
    ) -> Result<(), ErrorWithPayload<MidiOverdubInstruction>> {
        use ReadySubState::*;
        // TODO-medium Maybe we should start to play if not yet playing
        if let Playing(s) = self.state {
            supplier_chain.register_midi_overdub_mirror_source(args.mirror_source);
            self.state = Playing(PlayingState {
                overdubbing: true,
                ..s
            });
            Ok(())
        } else {
            Err(ErrorWithPayload::new("clip not playing", args))
        }
    }

    pub fn record(
        &mut self,
        args: ClipRecordArgs,
        project: Option<Project>,
        supplier_chain: &mut SupplierChain,
        audio_request_props: BasicAudioRequestProps,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
    ) -> Option<RecordingState> {
        let timeline = clip_timeline(project, false);
        let timeline_cursor_pos = timeline.cursor_pos();
        let tempo = timeline.tempo_at(timeline_cursor_pos);
        let initial_play_start_timing = column_settings
            .clip_play_start_timing
            .unwrap_or(matrix_settings.clip_play_start_timing);
        let timing = RecordTiming::from_args(
            &args,
            &timeline,
            timeline_cursor_pos,
            initial_play_start_timing,
        );
        let is_midi = args.recording_equipment.is_midi();
        let initial_pos = resolve_initial_recording_pos(
            timing,
            &timeline,
            timeline_cursor_pos,
            tempo,
            audio_request_props,
            is_midi,
        );
        let recording_args = RecordingArgs {
            equipment: args.recording_equipment,
            project,
            timeline_cursor_pos,
            tempo,
            time_signature: timeline.time_signature_at(timeline_cursor_pos),
            detect_downbeat: args.settings.downbeat_detection_enabled(is_midi),
            timing,
        };
        supplier_chain.prepare_recording(recording_args);
        let recording_state = RecordingState {
            pos: initial_pos,
            rollback_data: {
                let data = RollbackData {
                    play_settings: self.play_settings,
                };
                Some(data)
            },
            timing,
            settings: args.settings,
            initial_play_start_timing,
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

    pub fn seek(
        &mut self,
        desired_pos: UnitValue,
        supplier_chain: &SupplierChain,
    ) -> ClipEngineResult<()> {
        let material_info = supplier_chain.material_info()?;
        let frame_count = material_info.frame_count();
        let desired_frame = adjust_proportionally_positive(frame_count as f64, desired_pos.get());
        use ReadySubState::*;
        match self.state {
            Stopped | Suspending(_) => {}
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if supplier_chain.is_playing_already(pos) {
                        let up_cycled_frame =
                            self.up_cycle_frame(desired_frame, pos, frame_count, &material_info);
                        self.state = Playing(PlayingState {
                            seek_pos: Some(up_cycled_frame),
                            ..s
                        });
                    }
                }
            }
            Paused(s) => {
                let up_cycled_frame =
                    self.up_cycle_frame(desired_frame, s.pos, frame_count, &material_info);
                self.state = Paused(PausedState {
                    pos: up_cycled_frame as isize,
                });
            }
        }
        Ok(())
    }

    fn up_cycle_frame(
        &self,
        frame: usize,
        offset_pos: isize,
        frame_count: usize,
        material_info: &MaterialInfo,
    ) -> usize {
        let current_cycle = material_info.get_cycle_at_frame(offset_pos);
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
    pub fn stop<H: HandleStopEvent>(
        &mut self,
        args: ClipStopArgs,
        supplier_chain: &mut SupplierChain,
        event_handler: &H,
    ) -> ClipRecordingStopOutcome {
        use ClipRecordingStopOutcome::*;
        match self.timing {
            RecordTiming::Unsynced => {
                let ready_state = self.finish_recording(
                    None,
                    supplier_chain,
                    &args.timeline,
                    event_handler,
                    args.matrix_settings,
                    args.column_settings,
                );
                TransitionToReady(ready_state)
            }
            RecordTiming::Synced { start, end } => {
                let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
                let next_qp = args
                    .timeline
                    .next_quantized_pos_at(ref_pos, start.quantization());
                if next_qp.position() <= start.position() {
                    // Zero point of recording hasn't even been reached yet. Try to roll back.
                    if let Some(rollback_data) = &self.rollback_data {
                        // We have a previous source that we can roll back to.
                        supplier_chain.rollback_recording().unwrap();
                        event_handler.normal_recording_finished(NormalRecordingOutcome::Cancelled);
                        let ready_state = ReadyState {
                            state: ReadySubState::Stopped,
                            play_settings: rollback_data.play_settings,
                        };
                        TransitionToReady(ready_state)
                    } else {
                        // There was nothing to roll back to. How sad.
                        ClearSlot
                    }
                } else {
                    // We are recording already.
                    // TODO-high This is a bit weird, we change the settings ...
                    if end.is_some() {
                        // End already scheduled. Take care of stopping after recording.
                        self.settings.looped = false;
                    } else {
                        // End not scheduled yet. Schedule end.
                        supplier_chain.schedule_end_of_recording(next_qp, &args.timeline);
                        self.timing = RecordTiming::Synced {
                            start,
                            end: Some(next_qp),
                        };
                    }
                    KeepState
                }
            }
        }
    }

    fn process<H: HandleStopEvent>(
        &mut self,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
        event_handler: &H,
    ) -> Option<ReadyState> {
        // Advance recording position (for MIDI mainly)
        {
            let recording_info = supplier_chain
                .recording_info()
                .expect("no recording info available");
            let (source_frame_rate, ref_tempo) = if recording_info.is_midi {
                (MIDI_FRAME_RATE, MIDI_BASE_BPM)
            } else {
                (args.dest_sample_rate, recording_info.initial_tempo)
            };
            let num_source_frames = convert_duration_in_frames_to_other_frame_rate(
                args.dest_buffer.frame_count(),
                args.dest_sample_rate,
                source_frame_rate,
            );
            let tempo_factor = args.timeline_tempo.get() / ref_tempo.get();
            let tempo_adjusted_num_source_frames =
                adjust_proportionally_positive(num_source_frames as f64, tempo_factor);
            self.pos += tempo_adjusted_num_source_frames as isize;
        }
        // Process scheduled stop
        if let RecordTiming::Synced {
            start,
            end: Some(end),
        } = self.timing
        {
            let next_qp = args
                .timeline
                .next_quantized_pos_at(args.timeline_cursor_pos, end.quantization());
            if next_qp.position() >= end.position() {
                // Close to scheduled recording end.
                let block_length_in_timeline_frames = args.dest_buffer.frame_count();
                let timeline_frame_rate = args.dest_sample_rate;
                let block_length_in_secs = convert_duration_in_frames_to_seconds(
                    block_length_in_timeline_frames,
                    timeline_frame_rate,
                );
                let block_end_pos = args.timeline_cursor_pos + block_length_in_secs;
                let downbeat_pos = supplier_chain.downbeat_pos_during_recording(&args.timeline);
                let record_end_pos = args.timeline.pos_of_quantized_pos(end) - downbeat_pos;
                if block_end_pos >= record_end_pos {
                    // We have recorded the last block.
                    let ready_state = self.finish_recording(
                        Some((start, end)),
                        supplier_chain,
                        &args.timeline,
                        event_handler,
                        args.matrix_settings,
                        args.column_settings,
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

    fn finish_recording<H: HandleStopEvent>(
        self,
        start_and_end_pos: Option<(QuantizedPosition, QuantizedPosition)>,
        supplier_chain: &mut SupplierChain,
        timeline: &HybridTimeline,
        event_handler: &H,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
    ) -> ReadyState {
        debug!("Finishing recording");
        let outcome = supplier_chain.commit_recording(timeline).unwrap();
        let clip_settings = ProcessingRelevantClipSettings::derive_from_recording(
            &self.settings,
            &outcome.data,
            matrix_settings,
            column_settings,
            self.initial_play_start_timing,
        );
        let clip_settings = clip_settings.unwrap();
        let chain_settings = clip_settings.create_chain_settings(matrix_settings, column_settings);
        supplier_chain
            .configure_complete_chain(chain_settings)
            .unwrap();
        // Prepare ready state
        let ready_state = ReadyState {
            state: if clip_settings.looped {
                ReadySubState::Playing(PlayingState {
                    virtual_pos: match start_and_end_pos {
                        None => VirtualPosition::Now,
                        Some((_, end)) => VirtualPosition::Quantized(end),
                    },
                    ..Default::default()
                })
            } else {
                ReadySubState::Stopped
            },
            play_settings: clip_settings.create_play_settings(),
        };
        // Send event
        let committed_recording = CommittedRecording {
            kind_specific: outcome.kind_specific,
            clip_settings,
        };
        event_handler
            .normal_recording_finished(NormalRecordingOutcome::Committed(committed_recording));
        // Return ready state
        ready_state
    }
}

pub enum StopSlotInstruction {
    KeepSlot,
    ClearSlot,
}

enum ClipRecordingStopOutcome {
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
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a ColumnSettings,
}

impl<'a> ClipPlayArgs<'a> {
    pub fn resolve_start_timing(
        &self,
        clip_start_timing: Option<ClipPlayStartTiming>,
    ) -> ClipPlayStartTiming {
        clip_start_timing
            .or(self.column_settings.clip_play_start_timing)
            .unwrap_or(self.matrix_settings.clip_play_start_timing)
    }
}

#[derive(Debug)]
pub struct ClipStopArgs<'a> {
    pub stop_timing: Option<ClipPlayStopTiming>,
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    /// If this is `true` and the clip is overdubbing, it not just stops overdubbing but also
    /// playing the clip.
    pub enforce_play_stop: bool,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a ColumnSettings,
}

impl<'a> ClipStopArgs<'a> {
    pub fn resolve_start_timing(
        &self,
        clip_start_timing: Option<ClipPlayStartTiming>,
    ) -> ClipPlayStartTiming {
        clip_start_timing
            .or(self.column_settings.clip_play_start_timing)
            .unwrap_or(self.matrix_settings.clip_play_start_timing)
    }

    pub fn resolve_stop_timing(
        &self,
        clip_stop_timing: Option<ClipPlayStopTiming>,
    ) -> ClipPlayStopTiming {
        self.stop_timing
            .or(clip_stop_timing)
            .or(self.column_settings.clip_play_stop_timing)
            .unwrap_or(self.matrix_settings.clip_play_stop_timing)
    }
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

#[derive(Debug)]
pub enum SlotRecordInstruction {
    NewClip(RecordNewClipInstruction),
    ExistingClip(ClipRecordArgs),
    MidiOverdub(MidiOverdubInstruction),
}

#[derive(Debug)]
pub struct RecordNewClipInstruction {
    pub supplier_chain: SupplierChain,
    pub project: Option<Project>,
    pub shared_pos: SharedPos,
    pub timeline: HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub timing: RecordTiming,
    pub is_midi: bool,
    pub settings: MatrixClipRecordSettings,
    pub initial_play_start_timing: ClipPlayStartTiming,
}

#[derive(Debug)]
pub struct MidiOverdubInstruction {
    /// A clone of the current source into which the same overdub events are going to be written.
    ///
    /// Can then be sent back to the main thread so it can be saved correctly (without having to
    /// interfere with the real-time threads).  
    pub mirror_source: OwnedPcmSource,
}

#[derive(Debug)]
pub struct ClipRecordArgs {
    pub recording_equipment: RecordingEquipment,
    pub settings: MatrixClipRecordSettings,
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
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a ColumnSettings,
}

impl<'a, 'b> ClipProcessArgs<'a, 'b> {
    pub fn basic_audio_request_props(&self) -> BasicAudioRequestProps {
        BasicAudioRequestProps {
            block_length: self.dest_buffer.frame_count(),
            frame_rate: self.dest_sample_rate,
        }
    }
}

struct LogNaturalDeviationArgs<T: Timeline> {
    quantized_pos: QuantizedPosition,
    block_length: usize,
    timeline: T,
    timeline_cursor_pos: PositionInSeconds,
    // timeline_tempo: Bpm,
    clip_tempo_factor: f64,
    timeline_frame_rate: Hz,
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

pub fn calc_tempo_factor(clip_tempo: Bpm, timeline_tempo: Bpm) -> f64 {
    let timeline_tempo_factor = timeline_tempo.get() / clip_tempo.get();
    timeline_tempo_factor.max(MIN_TEMPO_FACTOR)
}

#[derive(Default)]
pub struct ClipPlayingOutcome {
    pub num_audio_frames_written: usize,
}

struct FillSamplesOutcome {
    clip_playing_outcome: ClipPlayingOutcome,
    next_frame: Option<isize>,
}

pub trait HandleStopEvent {
    fn midi_overdub_finished(&self, mirror_source: OwnedPcmSource);
    fn normal_recording_finished(&self, outcome: NormalRecordingOutcome);
}

/// Holds the result of a normal (non-overdub) recording.
///
/// Can also be cancelled.
#[derive(Clone, Debug)]
pub enum NormalRecordingOutcome {
    Committed(CommittedRecording),
    Cancelled,
}

/// Holds the data of a successful recording (material and settings).
#[derive(Clone, Debug)]
pub struct CommittedRecording {
    pub kind_specific: KindSpecificRecordingOutcome,
    pub clip_settings: ProcessingRelevantClipSettings,
}

/// All settings of a clip that affect processing.
///
/// To be sent back to the main thread to update the main thread clip.
#[derive(Clone, Debug)]
pub struct ProcessingRelevantClipSettings {
    pub time_base: api::ClipTimeBase,
    pub looped: bool,
    pub volume: api::Db,
    pub section: api::Section,
    pub start_timing: Option<api::ClipPlayStartTiming>,
    pub stop_timing: Option<api::ClipPlayStopTiming>,
    pub audio_settings: api::ClipAudioSettings,
    pub midi_settings: api::ClipMidiSettings,
}

impl ProcessingRelevantClipSettings {
    pub fn from_api(clip: &api::Clip) -> Self {
        Self {
            time_base: clip.time_base,
            looped: clip.looped,
            volume: clip.volume,
            section: clip.section,
            start_timing: clip.start_timing,
            stop_timing: clip.stop_timing,
            audio_settings: clip.audio_settings,
            midi_settings: clip.midi_settings,
        }
    }

    pub fn derive_from_recording(
        record_settings: &MatrixClipRecordSettings,
        data: &CompleteRecordingData,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
        initial_play_start_timing: ClipPlayStartTiming,
    ) -> ClipEngineResult<Self> {
        let current_play_start_timing = column_settings
            .clip_play_start_timing
            .unwrap_or(matrix_settings.clip_play_start_timing);
        let settings = Self {
            start_timing: record_settings
                .effective_play_start_timing(initial_play_start_timing, current_play_start_timing),
            stop_timing: record_settings
                .effective_play_stop_timing(initial_play_start_timing, current_play_start_timing),
            looped: record_settings.looped,
            time_base: {
                let audio_tempo = if data.is_midi {
                    None
                } else {
                    Some(api::Bpm::new(data.tempo.get())?)
                };
                record_settings.effective_play_time_base(
                    initial_play_start_timing,
                    audio_tempo,
                    api::TimeSignature {
                        numerator: data.time_signature.numerator.get(),
                        denominator: data.time_signature.denominator.get(),
                    },
                    api::PositiveBeat::new(data.downbeat_in_beats().get())?,
                )
            },
            volume: api::Db::ZERO,
            section: api::Section {
                start_pos: PositiveSecond::new(data.section_start_pos_in_seconds().get())?,
                length: data
                    .section_length_in_seconds()
                    .map(|l| PositiveSecond::new(l.get()).unwrap()),
            },
            audio_settings: ClipAudioSettings {
                // In general, a recording won't be automatically cut correctly, so we apply fades.
                apply_source_fades: true,
                time_stretch_mode: None,
                resample_mode: None,
                cache_behavior: None,
            },
            midi_settings: ClipMidiSettings::default(),
        };
        Ok(settings)
    }

    fn create_chain_settings(
        &self,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &ColumnSettings,
    ) -> ChainSettings {
        ChainSettings {
            looped: self.looped,
            time_base: self.time_base,
            volume: self.volume,
            section: self.section,
            audio_apply_source_fades: self.audio_settings.apply_source_fades,
            midi_settings: self.midi_settings,
            audio_time_stretch_mode: self
                .audio_settings
                .time_stretch_mode
                .or(column_settings.audio_time_stretch_mode)
                .unwrap_or(matrix_settings.audio_time_stretch_mode),
            audio_resample_mode: self
                .audio_settings
                .resample_mode
                .or(column_settings.audio_resample_mode)
                .unwrap_or(matrix_settings.audio_resample_mode),
            cache_behavior: self
                .audio_settings
                .cache_behavior
                .or(column_settings.audio_cache_behavior)
                .unwrap_or(matrix_settings.audio_cache_behavior),
        }
    }

    fn create_play_settings(&self) -> PlaySettings {
        PlaySettings {
            start_timing: self.start_timing,
            stop_timing: self.stop_timing,
            looped: self.looped,
            time_base: self.time_base,
        }
    }
}

fn log_natural_deviation(
    args: LogNaturalDeviationArgs<impl Timeline>,
    material_info: &MaterialInfo,
) {
    if args.quantized_pos.denominator() != 1 {
        // This is not a quantization to a single bar. Never tested with that.
        return;
    }
    let start_bar = args.quantized_pos.position() as i32;
    let start_bar_timeline_pos = args.timeline.pos_of_quantized_pos(args.quantized_pos);
    // Assuming a constant tempo and time signature during one cycle
    let clip_duration = material_info.duration();
    let source_frame_rate = material_info.frame_rate();
    let beat_count = calculate_beat_count(args.clip_tempo, clip_duration);
    let bar_count = (beat_count as f64 / 4.0).ceil() as u32;
    let end_bar = start_bar + bar_count as i32;
    let end_bar_timeline_pos = args.timeline.pos_of_bar(end_bar);
    debug_assert!(
        end_bar_timeline_pos > start_bar_timeline_pos,
        "end_bar_timeline_pos {} <= start_bar_timeline_pos {}",
        end_bar_timeline_pos,
        start_bar_timeline_pos
    );
    // Timeline cycle length
    let timeline_cycle_length_in_secs = (end_bar_timeline_pos - start_bar_timeline_pos).abs();
    let timeline_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
        timeline_cycle_length_in_secs,
        args.timeline_frame_rate,
    );
    let timeline_cycle_length_in_source_frames =
        convert_duration_in_seconds_to_frames(timeline_cycle_length_in_secs, source_frame_rate);
    // Source cycle length
    let source_cycle_length_in_secs = clip_duration;
    let source_cycle_length_in_timeline_frames = convert_duration_in_seconds_to_frames(
        source_cycle_length_in_secs,
        args.timeline_frame_rate,
    );
    let source_cycle_length_in_source_frames = material_info.frame_count();
    // Block length
    let block_length_in_timeline_frames = args.block_length;
    let block_length_in_secs = convert_duration_in_frames_to_seconds(
        block_length_in_timeline_frames,
        args.timeline_frame_rate,
    );
    let block_length_in_source_frames = convert_duration_in_frames_to_other_frame_rate(
        block_length_in_timeline_frames,
        args.timeline_frame_rate,
        source_frame_rate,
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
    let adjusted_block_length_in_timeline_frames = convert_duration_in_frames_to_other_frame_rate(
        adjusted_block_length_in_source_frames,
        source_frame_rate,
        args.timeline_frame_rate,
    );
    let adjusted_block_length_in_secs = convert_duration_in_frames_to_seconds(
        adjusted_block_length_in_source_frames,
        source_frame_rate,
    );
    let adjusted_remainder_in_source_frames =
        adjust_proportionally_positive(remainder_in_source_frames as f64, args.clip_tempo_factor);
    // Source cycle remainder
    let adjusted_remainder_in_timeline_frames = convert_duration_in_frames_to_other_frame_rate(
        adjusted_remainder_in_source_frames,
        source_frame_rate,
        args.timeline_frame_rate,
    );
    let adjusted_remainder_in_secs = convert_duration_in_frames_to_seconds(
        adjusted_remainder_in_source_frames,
        source_frame_rate,
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
        start_bar,
        end_bar,
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

fn resolve_initial_recording_pos(
    timing: RecordTiming,
    timeline: &HybridTimeline,
    timeline_cursor_pos: PositionInSeconds,
    timeline_tempo: Bpm,
    audio_request_props: BasicAudioRequestProps,
    is_midi: bool,
) -> isize {
    match timing {
        RecordTiming::Unsynced => 0,
        RecordTiming::Synced { start, .. } => {
            let equipment = QuantizedPosCalcEquipment {
                audio_request_props,
                timeline: &timeline,
                timeline_cursor_pos,
                clip_tempo_factor: if is_midi {
                    timeline_tempo.get() / MIDI_BASE_BPM.get()
                } else {
                    1.0
                },
                source_frame_rate: if is_midi {
                    MIDI_FRAME_RATE
                } else {
                    audio_request_props.frame_rate
                },
            };
            calc_distance_from_quantized_pos(start, equipment)
        }
    }
}

fn resolve_virtual_pos(
    virtual_pos: VirtualPosition,
    process_args: &ClipProcessArgs,
    clip_tempo_factor: f64,
    log_natural_deviation_enabled: bool,
    material_info: &MaterialInfo,
    // Used for logging natural deviation.
    clip_tempo: Option<Bpm>,
) -> isize {
    use VirtualPosition::*;
    match virtual_pos {
        Now => 0,
        Quantized(qp) => {
            let equipment = QuantizedPosCalcEquipment {
                audio_request_props: process_args.basic_audio_request_props(),
                timeline: process_args.timeline,
                timeline_cursor_pos: process_args.timeline_cursor_pos,
                clip_tempo_factor,
                source_frame_rate: material_info.frame_rate(),
            };
            let pos = calc_distance_from_quantized_pos(qp, equipment);
            if log_natural_deviation_enabled {
                // Quantization to bar
                if let Some(clip_tempo) = clip_tempo {
                    // Plus, we react to tempo changes.
                    let args = LogNaturalDeviationArgs {
                        quantized_pos: qp,
                        block_length: process_args.dest_buffer.frame_count(),
                        timeline: process_args.timeline,
                        timeline_cursor_pos: process_args.timeline_cursor_pos,
                        clip_tempo_factor,
                        timeline_frame_rate: process_args.dest_sample_rate,
                        clip_tempo,
                    };
                    log_natural_deviation(args, material_info);
                }
            }
            pos
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BasicAudioRequestProps {
    pub block_length: usize,
    pub frame_rate: Hz,
}

impl BasicAudioRequestProps {
    pub fn from_transfer(transfer: &PcmSourceTransfer) -> Self {
        Self {
            block_length: transfer.length() as _,
            frame_rate: transfer.sample_rate(),
        }
    }
}

pub struct QuantizedPosCalcEquipment<'a> {
    pub audio_request_props: BasicAudioRequestProps,
    pub timeline: &'a HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub clip_tempo_factor: f64,
    pub source_frame_rate: Hz,
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
    quantized_pos: QuantizedPosition,
    equipment: QuantizedPosCalcEquipment,
) -> isize {
    // Essential calculation
    let quantized_timeline_pos = equipment.timeline.pos_of_quantized_pos(quantized_pos);
    let rel_pos_from_quant_in_secs = equipment.timeline_cursor_pos - quantized_timeline_pos;
    let rel_pos_from_quant_in_source_frames = convert_position_in_seconds_to_frames(
        rel_pos_from_quant_in_secs,
        equipment.source_frame_rate,
    );
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
        equipment.audio_request_props.block_length,
        equipment.audio_request_props.frame_rate,
        equipment.source_frame_rate,
    );
    adjust_proportionally_in_blocks(
        rel_pos_from_quant_in_source_frames,
        equipment.clip_tempo_factor,
        block_length_in_source_frames,
    )
}

fn modulo_frame(frame: isize, frame_count: usize) -> isize {
    if frame < 0 {
        frame
    } else {
        frame % frame_count as isize
    }
}
