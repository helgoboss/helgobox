use crate::base::{ClipAddress, ClipSlotAddress};
use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_other_frame_rate,
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
};
use crate::rt::buffer::AudioBufMut;
use crate::rt::schedule_util::calc_distance_from_quantized_pos;
use crate::rt::supplier::{
    AudioSupplier, ChainEquipment, ChainSettings, CompleteRecordingData,
    KindSpecificRecordingOutcome, MaterialInfo, MidiOverdubOutcome, MidiOverdubSettings,
    MidiSequence, MidiSupplier, PollRecordingOutcome, ReaperClipSource, RecordState, Recorder,
    RecorderRequest, RecordingArgs, RecordingEquipment, RecordingOutcome, RtClipSource,
    StopRecordingOutcome, SupplierChain, SupplyAudioRequest, SupplyMidiRequest,
    SupplyRequestGeneralInfo, SupplyRequestInfo, SupplyResponse, SupplyResponseStatus,
    WithMaterialInfo, WriteAudioRequest, WriteMidiRequest, MIDI_BASE_BPM, MIDI_FRAME_RATE,
};
use crate::rt::tempo_util::{calc_tempo_factor, determine_tempo_from_time_base};
use crate::rt::{OverridableMatrixSettings, RtClips, RtColumnEvent, RtColumnSettings};
use crate::timeline::{HybridTimeline, Timeline};
use crate::{ClipEngineResult, ErrorWithPayload, Laziness, QuantizedPosition};
use atomic::Atomic;
use crossbeam_channel::Sender;
use helgoboss_learn::UnitValue;
use helgoboss_midi::ShortMessage;
use playtime_api::persistence as api;
use playtime_api::persistence::{
    ClipAudioSettings, ClipId, ClipPlayStartTiming, ClipPlayStopTiming, ClipTimeBase, Db,
    EvenQuantization, MatrixClipRecordSettings, PositiveSecond,
};
use playtime_api::runtime::ClipPlayState;
use reaper_high::Project;
use reaper_medium::{
    BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, OnAudioBufferArgs, PcmSourceTransfer,
    PositionInSeconds,
};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct RtClipId(u64);

impl RtClipId {
    pub fn from_clip_id(clip_id: &ClipId) -> Self {
        Self(base::hash_util::calculate_non_crypto_hash(clip_id))
    }
}

#[derive(Debug)]
pub struct RtClip {
    id: RtClipId,
    supplier_chain: SupplierChain,
    state: ClipState,
    project: Option<Project>,
    shared_pos: SharedPos,
    shared_peak: SharedPeak,
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

#[derive(Copy, Clone, Debug)]
struct ReadyState {
    state: ReadySubState,
    settings: RtClipSettings,
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

    fn set(&self, pos: isize) {
        self.0.store(pos, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, Default)]
pub struct SharedPeak(Arc<Atomic<UnitValue>>);

impl SharedPeak {
    /// Returns the last detected peak value. Plus, if a MIDI note-on was encountered
    /// (= peak is MAX), resets value to MIN in order to acknowledge receipt of the note-on event.
    pub fn reset(&self) -> UnitValue {
        let res = self.0.compare_exchange(
            UnitValue::MAX,
            UnitValue::MIN,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        match res {
            Ok(v) => v,
            Err(v) => v,
        }
    }

    fn set(&self, peak: UnitValue) {
        self.0.store(peak, Ordering::Relaxed);
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
    rollback_data: Option<RollbackData>,
    settings: MatrixClipRecordSettings,
}

#[derive(Copy, Clone, Debug)]
struct RollbackData {
    clip_settings: RtClipSettings,
}

impl RtClip {
    /// Must not call in real-time thread!
    #[allow(clippy::too_many_arguments)]
    pub fn ready(
        id: RtClipId,
        pcm_source: RtClipSource,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
        clip_settings: RtClipSettings,
        permanent_project: Option<Project>,
        chain_equipment: &ChainEquipment,
        recorder_request_sender: &Sender<RecorderRequest>,
    ) -> ClipEngineResult<Self> {
        let mut supplier_chain = SupplierChain::new(
            Recorder::ready(pcm_source, recorder_request_sender.clone()),
            chain_equipment.clone(),
        )?;
        let chain_settings = clip_settings.create_chain_settings(matrix_settings, column_settings);
        supplier_chain.configure_complete_chain(chain_settings)?;
        supplier_chain.pre_buffer_simple(0);
        let ready_state = ReadyState {
            state: ReadySubState::Stopped,
            settings: clip_settings,
        };
        let clip = Self {
            id,
            supplier_chain,
            state: ClipState::Ready(ready_state),
            project: permanent_project,
            shared_pos: Default::default(),
            shared_peak: Default::default(),
        };
        Ok(clip)
    }

    pub fn recording(instruction: RecordNewClipInstruction) -> Self {
        let recording_state = RecordingState {
            rollback_data: None,
            settings: instruction.settings,
        };
        instruction.supplier_chain.emit_audio_recording_task();
        Self {
            id: instruction.clip_id,
            supplier_chain: instruction.supplier_chain,
            state: ClipState::Recording(recording_state),
            project: instruction.project,
            shared_pos: instruction.shared_pos,
            shared_peak: instruction.shared_peak,
        }
    }

    pub fn id(&self) -> RtClipId {
        self.id
    }

    /// Applies properties of the given clip to this clip.
    ///
    /// This gets called when the slot load logic detects an existing clip with the same ID.
    /// Instead of replacing the slot's clip with the new clip, it lets the old clip (this one!)
    /// apply the properties of the new clip to itself. This makes it possible to keep the clip
    /// playing, applying just the differences.
    ///
    /// # Errors
    ///
    /// If this clip or the given clip is recording or material doesn't deliver info.
    pub fn apply(&mut self, args: ApplyClipArgs) -> ClipEngineResult<()> {
        let (ClipState::Ready(this_clip), ClipState::Ready(new_clip))= (&mut self.state, &mut args.other_clip.state) else {
            return Err("can't apply if this or given clip is in recording state");
        };
        let setting_args = SetClipSettingsArgs {
            clip_settings: new_clip.settings,
            matrix_settings: args.matrix_settings,
            column_settings: args.column_settings,
        };
        this_clip.set_settings(setting_args, &mut self.supplier_chain)?;
        // Really important to reconnect the shared position and peak info variables, otherwise the
        // UI will not display them anymore.
        self.shared_pos = args.other_clip.shared_pos.clone();
        self.shared_peak = args.other_clip.shared_peak.clone();
        Ok(())
    }

    /// If recording, delivers material info of the material that's being recorded.
    pub fn recording_material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier_chain.recording_material_info()
    }

    /// Plays the clip if it's not recording.
    pub fn play(&mut self, args: SlotPlayArgs) -> ClipEngineResult<PlayOutcome> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => Ok(s.play(args, &mut self.supplier_chain)),
            Recording(_) => Err("recording"),
        }
    }

    /// Stops the clip immediately, initiating fade-outs if necessary.
    ///
    /// Also stops clip recording. Consumer should just wait for the clip to be stopped and then not
    /// use it anymore.
    pub fn initiate_removal(&mut self) {
        match &mut self.state {
            ClipState::Ready(s) => s.panic(&mut self.supplier_chain),
            ClipState::Recording(_) => {
                self.state = ClipState::Ready(ReadyState {
                    state: ReadySubState::Stopped,
                    settings: Default::default(),
                })
            }
        }
    }

    /// Stops the clip immediately, initiating fade-outs if necessary.
    ///
    /// Doesn't stop a clip recording.
    pub fn panic(&mut self) {
        match &mut self.state {
            ClipState::Ready(s) => s.panic(&mut self.supplier_chain),
            ClipState::Recording(_) => {}
        }
    }

    /// Stops the clip playing or recording.
    pub fn stop<H: HandleSlotEvent>(
        &mut self,
        args: SlotStopArgs,
        event_handler: &H,
    ) -> ClipEngineResult<Option<SlotInstruction>> {
        use ClipState::*;
        let instruction = match &mut self.state {
            Ready(s) => {
                if let Some(outcome) = s.stop(args, &mut self.supplier_chain) {
                    event_handler.midi_overdub_finished(self.id, outcome);
                }
                None
            }
            Recording(s) => {
                use ClipRecordingStopOutcome::*;
                match s.stop(args, &mut self.supplier_chain, event_handler)? {
                    KeepState => None,
                    TransitionToReady(ready_state) => {
                        self.state = Ready(ready_state);
                        None
                    }
                    ClearSlot => Some(SlotInstruction::ClearSlot),
                }
            }
        };
        Ok(instruction)
    }

    pub fn set_settings(&mut self, args: SetClipSettingsArgs) -> ClipEngineResult<()> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => s.set_settings(args, &mut self.supplier_chain),
            Recording(_) => Err("can't set settings while recording"),
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

    pub fn set_section(&mut self, section: api::Section) -> ClipEngineResult<()> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                s.set_section(section, &mut self.supplier_chain);
                Ok(())
            }
            Recording(_) => Err("can't set section while recording"),
        }
    }

    pub fn looped(&self) -> bool {
        use ClipState::*;
        match self.state {
            Ready(s) => s.settings.looped,
            Recording(_) => false,
        }
    }

    // TODO-high-clip-engine The error type is too large!
    #[allow(clippy::result_large_err)]
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

    // TODO-high-clip-engine The error type is too large!
    #[allow(clippy::result_large_err)]
    pub fn record(
        &mut self,
        args: ClipRecordArgs,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
    ) -> Result<(), ErrorWithPayload<ClipRecordArgs>> {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                let new_state = s.record(
                    args,
                    self.project,
                    &mut self.supplier_chain,
                    matrix_settings,
                    column_settings,
                );
                self.supplier_chain.emit_audio_recording_task();
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

    /// Should be called exactly once per block when recording and before writing material,
    /// in order to drive various record-related processing and also to know when to stop polling
    /// and writing material.
    ///
    /// Returns `false` if not necessary to poll and write material anymore.
    pub fn recording_poll<H: HandleSlotEvent>(
        &mut self,
        args: ClipRecordingPollArgs,
        event_handler: &H,
    ) -> bool {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => match &s.state {
                ReadySubState::Playing(s) => s.overdubbing,
                _ => false,
            },
            Recording(s) => {
                use PollRecordingOutcome::*;
                match self.supplier_chain.poll_recording(args.audio_request_props) {
                    PleaseStopPolling => false,
                    CommittedRecording(outcome) => {
                        let ready_state = s.finish_recording(
                            outcome,
                            &mut self.supplier_chain,
                            event_handler,
                            args.matrix_settings,
                            args.column_settings,
                        );
                        self.state = Ready(ready_state);
                        false
                    }
                    PleaseContinuePolling { pos } => {
                        self.shared_pos.set(pos);
                        // TODO-high-clip-engine Set recording peak somewhere
                        true
                    }
                }
            }
        }
    }

    /// Writes the events in the given request into the currently recording MIDI source.
    pub fn write_midi(&mut self, request: WriteMidiRequest) {
        use ClipState::*;
        let play_pos = match &self.state {
            Ready(s) => match s.state {
                ReadySubState::Playing(PlayingState {
                    overdubbing: true,
                    pos: Some(pos),
                    ..
                }) => Some(pos),
                _ => return,
            },
            Recording(_) => None,
        };
        self.supplier_chain.write_midi(request, play_pos).unwrap();
    }

    /// Writes the samples in the given request into the currently recording audio source.
    ///
    /// Also drives processing during recording because it's called exactly once per audio block
    /// anyway.
    pub fn write_audio(&mut self, request: impl WriteAudioRequest) {
        self.supplier_chain.write_audio(request);
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.supplier_chain.set_volume(volume);
    }

    pub fn shared_pos(&self) -> SharedPos {
        self.shared_pos.clone()
    }

    pub fn shared_peak(&self) -> SharedPeak {
        self.shared_peak.clone()
    }

    /// Attention: If this returns some info while in the middle of recording, this returns
    /// information about the previous clip's material! Use [`Self::recording_material_info`]
    /// instead if you need to query information about the material that's being recorded.
    pub fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier_chain.material_info()
    }

    /// Returns the current clip play state.
    ///
    /// Attention: If the clip is being suspended (e.g. fading out), this will return the state
    /// after suspension, e.g. "Stopped". So  don't use this to check whether processing is still
    /// necessary.
    pub fn play_state(&self) -> InternalClipPlayState {
        match &self.state {
            ClipState::Ready(s) => s.play_state(),
            ClipState::Recording(_) => {
                use RecordState::*;
                let api_state = match self
                    .supplier_chain
                    .record_state()
                    .expect("recorder not recording while clip recording")
                {
                    ScheduledForStart => ClipPlayState::ScheduledForRecordingStart,
                    Recording => ClipPlayState::Recording,
                    ScheduledForStop => ClipPlayState::ScheduledForRecordingStop,
                };
                api_state.into()
            }
        }
    }

    pub fn process(&mut self, args: &mut ClipProcessArgs) -> ClipProcessingOutcome {
        use ClipState::*;
        match &mut self.state {
            Ready(s) => {
                let (outcome, changed_state) = s.process(
                    args,
                    &mut self.supplier_chain,
                    &mut self.shared_pos,
                    &mut self.shared_peak,
                );
                if let Some(s) = changed_state {
                    debug!("Changing to recording state {:?}", &s);
                    self.state = Recording(s);
                }
                outcome
            }
            Recording(_) => {
                // Recording is not driven by the preview register processing but uses a separate
                // record polling which is driven by the code that provides the input material.
                ClipProcessingOutcome::default()
            }
        }
    }
}

impl ReadyState {
    /// Stops the clip immediately, initiating fade-outs if necessary.
    pub fn panic(&mut self, supplier_chain: &mut SupplierChain) {
        use ReadySubState::*;
        self.state = match self.state {
            Playing(PlayingState { pos: Some(pos), .. }) => {
                // Processing will automatically install an immediate stop interaction
                // when entering the suspending state and there's no stop interaction yet.
                // However, it will not do this when a stop interaction is installed already, e.g.
                // a scheduled one (clip has scheduled stop). So we need to enforce an immediate
                // one.
                supplier_chain.install_immediate_stop_interaction(pos);
                Suspending(SuspendingState {
                    next_state: StateAfterSuspension::Stopped,
                    pos,
                })
            }
            Suspending(s) => Suspending(SuspendingState {
                next_state: StateAfterSuspension::Stopped,
                ..s
            }),
            _ => Stopped,
        };
    }

    /// Returns `None` if time base is not "Beat".
    fn tempo(&self, is_midi: bool) -> Option<Bpm> {
        determine_tempo_from_time_base(&self.settings.time_base, is_midi)
    }

    pub fn set_settings(
        &mut self,
        args: SetClipSettingsArgs,
        supplier_chain: &mut SupplierChain,
    ) -> ClipEngineResult<()> {
        self.settings = args.clip_settings;
        let material_info = supplier_chain.material_info()?;
        let chain_settings = args
            .clip_settings
            .create_chain_settings(args.matrix_settings, args.column_settings);
        self.set_time_base(chain_settings.time_base, supplier_chain, &material_info);
        self.set_looped(chain_settings.looped, supplier_chain);
        self.set_section(chain_settings.section, supplier_chain);
        self.set_start_timing(args.clip_settings.start_timing);
        self.set_stop_timing(args.clip_settings.stop_timing);
        supplier_chain.set_volume(chain_settings.volume);
        supplier_chain.set_audio_fades_enabled_for_source(chain_settings.audio_apply_source_fades);
        supplier_chain.set_audio_time_stretch_mode(chain_settings.audio_time_stretch_mode);
        supplier_chain.set_audio_resample_mode(chain_settings.audio_resample_mode);
        supplier_chain.set_audio_cache_behavior(chain_settings.cache_behavior);
        supplier_chain.set_midi_settings(args.clip_settings.midi_settings);
        Ok(())
    }

    fn set_time_base(
        &mut self,
        time_base: ClipTimeBase,
        supplier_chain: &mut SupplierChain,
        material_info: &MaterialInfo,
    ) {
        self.settings.time_base = time_base;
        supplier_chain.set_time_base(&time_base, material_info);
    }

    fn set_start_timing(&mut self, start_timing: Option<ClipPlayStartTiming>) {
        self.settings.start_timing = start_timing;
    }

    fn set_stop_timing(&mut self, stop_timing: Option<ClipPlayStopTiming>) {
        self.settings.stop_timing = stop_timing;
    }

    pub fn set_looped(&mut self, looped: bool, supplier_chain: &mut SupplierChain) {
        self.settings.looped = looped;
        if !looped {
            if let ReadySubState::Playing(PlayingState { pos: Some(pos), .. }) = self.state {
                supplier_chain.keep_playing_until_end_of_current_cycle(pos);
                return;
            }
        }
        supplier_chain.set_looped(self.settings.looped);
    }

    pub fn set_section(&mut self, section: api::Section, supplier_chain: &mut SupplierChain) {
        supplier_chain.set_section(section.start_pos, section.length);
    }

    pub fn play(&mut self, args: SlotPlayArgs, supplier_chain: &mut SupplierChain) -> PlayOutcome {
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
                let pos = s.pos;
                supplier_chain.install_immediate_start_interaction(pos);
                self.state = ReadySubState::Playing(PlayingState {
                    pos: Some(pos),
                    ..Default::default()
                });
            }
        }
        PlayOutcome { virtual_pos }
    }

    fn resolve_stop_timing(&self, stop_args: &SlotStopArgs) -> ConcreteClipPlayStopTiming {
        let start_timing = stop_args.resolve_start_timing(self.settings.start_timing);
        let stop_timing = stop_args.resolve_stop_timing(self.settings.stop_timing);
        ConcreteClipPlayStopTiming::resolve(start_timing, stop_timing)
    }

    fn calculate_virtual_play_pos(&self, play_args: &SlotPlayArgs) -> VirtualPosition {
        let start_timing = play_args.resolve_start_timing(self.settings.start_timing);
        use ClipPlayStartTiming::*;
        match start_timing {
            Immediately => VirtualPosition::Now,
            Quantized(q) => {
                let ref_pos = play_args
                    .ref_pos
                    .unwrap_or_else(|| play_args.timeline.cursor_pos());
                let quantized_pos = play_args.timeline.next_quantized_pos_at(
                    ref_pos,
                    q,
                    Laziness::DwellingOnCurrentPos,
                );
                VirtualPosition::Quantized(quantized_pos)
            }
        }
    }

    /// Stops the clip.
    ///
    /// By default, if it's overdubbing, it just stops the overdubbing (a second call will make
    /// it stop playing).
    pub fn stop(
        &mut self,
        args: SlotStopArgs,
        supplier_chain: &mut SupplierChain,
    ) -> Option<MidiOverdubOutcome> {
        use ReadySubState::*;
        match self.state {
            Stopped => None,
            Playing(s) => {
                let overdub_outcome = if s.overdubbing {
                    // Currently recording overdub. Stop recording.
                    self.state = Playing(PlayingState {
                        overdubbing: false,
                        ..s
                    });
                    let outcome = supplier_chain.stop_midi_overdubbing().ok();
                    if !args.enforce_play_stop {
                        // Continue playing
                        return outcome;
                    }
                    outcome
                } else {
                    None
                };
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
                                    let quantized_pos = args.timeline.next_quantized_pos_at(
                                        ref_pos,
                                        q,
                                        Laziness::DwellingOnCurrentPos,
                                    );
                                    Playing(PlayingState {
                                        stop_request: Some(StopRequest::Quantized(quantized_pos)),
                                        ..s
                                    })
                                }
                                UntilEndOfClip => {
                                    if self.settings.looped {
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
                overdub_outcome
            }
            Paused(_) => {
                self.state = Stopped;
                None
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
                None
            }
        }
    }

    pub fn process(
        &mut self,
        args: &mut ClipProcessArgs,
        supplier_chain: &mut SupplierChain,
        shared_pos: &mut SharedPos,
        shared_peak: &mut SharedPeak,
    ) -> (ClipProcessingOutcome, Option<RecordingState>) {
        use ReadySubState::*;
        let (outcome, changed_state, pos) = match self.state {
            Stopped | Paused(_) => return (Default::default(), None),
            Playing(s) => {
                let outcome = self.process_playing(s, args, supplier_chain, shared_peak);
                (outcome, None, s.pos.unwrap_or_default())
            }
            Suspending(s) => {
                let (outcome, changed_state) =
                    self.process_suspending(s, args, supplier_chain, shared_peak);
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
        shared_peak: &mut SharedPeak,
    ) -> ClipProcessingOutcome {
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
        let fill_samples_outcome = self.fill_samples(
            args,
            go.pos,
            &general_info,
            go.sample_rate_factor,
            supplier_chain,
            &material_info,
            shared_peak,
        );
        self.state = if let Some(next_frame) = fill_samples_outcome.next_frame {
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
        ClipProcessingOutcome {
            num_audio_frames_written: fill_samples_outcome.num_audio_frames_written,
        }
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
    #[allow(clippy::too_many_arguments)]
    fn fill_samples(
        &mut self,
        args: &mut ClipProcessArgs,
        start_frame: isize,
        info: &SupplyRequestGeneralInfo,
        sample_rate_factor: f64,
        supplier_chain: &mut SupplierChain,
        material_info: &MaterialInfo,
        shared_peak: &mut SharedPeak,
    ) -> FillSamplesOutcome {
        let dest_sample_rate = Hz::new(args.dest_sample_rate.get() * sample_rate_factor);
        let is_midi = material_info.is_midi();
        let response = if is_midi {
            let resp =
                self.fill_samples_midi(args, start_frame, info, dest_sample_rate, supplier_chain);
            let has_note_on = args
                .midi_event_list
                .iter()
                .any(|evt| evt.message().is_note_on());
            if has_note_on {
                // Main thread is responsible for setting it back to MIN (acknowledges reading).
                shared_peak.set(UnitValue::MAX);
            }
            resp
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
            num_audio_frames_written: if is_midi { 0 } else { num_frames_written },
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
        shared_peak: &mut SharedPeak,
    ) -> (ClipProcessingOutcome, Option<RecordingState>) {
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
        let fill_samples_outcome = self.fill_samples(
            args,
            s.pos,
            &general_info,
            1.0,
            supplier_chain,
            &material_info,
            shared_peak,
        );
        let (next_state, recording_state) =
            if let Some(next_frame) = fill_samples_outcome.next_frame {
                // Suspension not finished yet.
                let next_state = ReadySubState::Suspending(SuspendingState {
                    pos: next_frame,
                    ..s
                });
                (next_state, None)
            } else {
                // Suspension finished.
                use StateAfterSuspension::*;
                self.reset_for_play(supplier_chain);
                match s.next_state {
                    Playing(s) => (ReadySubState::Playing(s), None),
                    Paused => (ReadySubState::Paused(PausedState { pos: s.pos }), None),
                    Stopped => {
                        supplier_chain.pre_buffer_simple(0);
                        (ReadySubState::Stopped, None)
                    }
                    Recording(s) => (self.state, Some(s)),
                }
            };
        self.state = next_state;
        let outcome = ClipProcessingOutcome {
            num_audio_frames_written: fill_samples_outcome.num_audio_frames_written,
        };
        (outcome, recording_state)
    }

    fn reset_for_play(&mut self, supplier_chain: &mut SupplierChain) {
        supplier_chain.reset_for_play(self.settings.looped);
    }

    // TODO-high-clip-engine The error type is too large!
    #[allow(clippy::result_large_err)]
    pub fn midi_overdub(
        &mut self,
        args: MidiOverdubInstruction,
        supplier_chain: &mut SupplierChain,
    ) -> Result<(), ErrorWithPayload<MidiOverdubInstruction>> {
        use ReadySubState::*;
        // TODO-medium Maybe we should start to play if not yet playing
        if let Playing(s) = self.state {
            supplier_chain.start_midi_overdub(args.source_replacement, args.settings);
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
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
    ) -> Option<RecordingState> {
        let recording_args = RecordingArgs::from_stuff(
            project,
            column_settings,
            matrix_settings,
            &args.settings,
            args.recording_equipment,
        );
        supplier_chain.prepare_recording(recording_args);
        let recording_state = RecordingState {
            rollback_data: {
                let data = RollbackData {
                    clip_settings: self.settings,
                };
                Some(data)
            },
            settings: args.settings,
        };
        use ReadySubState::*;
        match self.state {
            Stopped => Some(recording_state),
            Playing(s) => {
                if let Some(pos) = s.pos {
                    if supplier_chain.is_playing_already(pos) {
                        debug!("Suspending play in order to start recording");
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

    pub fn play_state(&self) -> InternalClipPlayState {
        use ReadySubState::*;
        let api_state = match self.state {
            Stopped => ClipPlayState::Stopped,
            Playing(s) => {
                if s.overdubbing {
                    ClipPlayState::Recording
                } else if s.stop_request.is_some() {
                    ClipPlayState::ScheduledForPlayStop
                } else if let Some(pos) = s.pos {
                    // It's correct that we don't consider the downbeat here. We want to expose
                    // the count-in phase as count-in phase, even some pickup beats are playing
                    // already.
                    if pos < 0 {
                        ClipPlayState::ScheduledForPlayStart
                    } else {
                        ClipPlayState::Playing
                    }
                } else {
                    ClipPlayState::ScheduledForPlayStart
                }
            }
            Suspending(s) => match s.next_state {
                StateAfterSuspension::Playing(_) => ClipPlayState::Playing,
                StateAfterSuspension::Paused => ClipPlayState::Paused,
                StateAfterSuspension::Stopped => ClipPlayState::Stopped,
                StateAfterSuspension::Recording(_) => ClipPlayState::Recording,
            },
            Paused(_) => ClipPlayState::Paused,
        };
        api_state.into()
    }

    fn schedule_play_internal(&mut self, virtual_pos: VirtualPosition) {
        self.state = ReadySubState::Playing(PlayingState {
            virtual_pos,
            ..Default::default()
        });
    }
}

impl RecordingState {
    pub fn stop<H: HandleSlotEvent>(
        &mut self,
        args: SlotStopArgs,
        supplier_chain: &mut SupplierChain,
        event_handler: &H,
    ) -> ClipEngineResult<ClipRecordingStopOutcome> {
        let ref_pos = args.ref_pos.unwrap_or_else(|| args.timeline.cursor_pos());
        let outcome = match supplier_chain.stop_recording(
            args.timeline,
            ref_pos,
            args.audio_request_props,
        )? {
            StopRecordingOutcome::Committed(outcome) => {
                let ready_state = self.finish_recording(
                    outcome,
                    supplier_chain,
                    event_handler,
                    args.matrix_settings,
                    args.column_settings,
                );
                ClipRecordingStopOutcome::TransitionToReady(ready_state)
            }
            StopRecordingOutcome::Canceled => {
                event_handler.normal_recording_finished(NormalRecordingOutcome::Canceled);
                if let Some(rollback_data) = &self.rollback_data {
                    let ready_state = ReadyState {
                        state: ReadySubState::Stopped,
                        settings: rollback_data.clip_settings,
                    };
                    debug!("Rolling back to old clip");
                    ClipRecordingStopOutcome::TransitionToReady(ready_state)
                } else {
                    debug!("Clearing slot after recording canceled");
                    ClipRecordingStopOutcome::ClearSlot
                }
            }
            StopRecordingOutcome::EndScheduled => ClipRecordingStopOutcome::KeepState,
        };
        Ok(outcome)
    }

    fn finish_recording<H: HandleSlotEvent>(
        self,
        outcome: RecordingOutcome,
        supplier_chain: &mut SupplierChain,
        event_handler: &H,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
    ) -> ReadyState {
        debug!("Finishing recording");
        let clip_settings = RtClipSettings::derive_from_recording(
            &self.settings,
            &outcome.data,
            matrix_settings,
            column_settings,
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
                    virtual_pos: match outcome.data.section_and_downbeat_data.quantized_end_pos {
                        None => VirtualPosition::Now,
                        Some(qp) => VirtualPosition::Quantized(qp),
                    },
                    ..Default::default()
                })
            } else {
                ReadySubState::Stopped
            },
            settings: clip_settings,
        };
        // Send event
        let material_info = outcome.material_info();
        let committed_recording = CommittedRecording {
            kind_specific: outcome.kind_specific,
            clip_settings,
            material_info,
        };
        event_handler
            .normal_recording_finished(NormalRecordingOutcome::Committed(committed_recording));
        // Return ready state
        // Finishing recording happens in the call stack of either record polling or stopping.
        // Both of these things happen *before* get_samples() is called by the preview register.
        // So get_samples() for the same block as the one we are in now will be called a moment
        // later. That's what guarantees us that we don't miss any samples.
        ready_state
    }
}

pub enum SlotInstruction {
    ClearSlot,
}

#[allow(clippy::large_enum_variant)]
enum ClipRecordingStopOutcome {
    KeepState,
    TransitionToReady(ReadyState),
    ClearSlot,
}

#[derive(Copy, Clone, Debug)]
pub struct SlotPlayArgs<'a> {
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to play a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
    pub start_timing: Option<ClipPlayStartTiming>,
}

#[derive(Debug)]
pub struct SlotLoadArgs<'a> {
    pub new_clips: RtClips,
    pub event_sender: &'a Sender<RtColumnEvent>,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
}

impl<'a> SlotPlayArgs<'a> {
    pub fn resolve_start_timing(
        &self,
        clip_start_timing: Option<ClipPlayStartTiming>,
    ) -> ClipPlayStartTiming {
        self.start_timing
            .or(clip_start_timing)
            .or(self.column_settings.clip_play_start_timing)
            .unwrap_or(self.matrix_settings.clip_play_start_timing)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SlotStopArgs<'a> {
    pub stop_timing: Option<ClipPlayStopTiming>,
    pub timeline: &'a HybridTimeline,
    /// Set this if you already have the current timeline position or want to stop a batch of clips.
    pub ref_pos: Option<PositionInSeconds>,
    /// If this is `true` and the clip is overdubbing, it not just stops overdubbing but also
    /// playing the clip.
    pub enforce_play_stop: bool,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
    pub audio_request_props: BasicAudioRequestProps,
}

#[derive(Debug)]
pub struct ClipRecordingPollArgs<'a> {
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
    pub audio_request_props: BasicAudioRequestProps,
}

impl<'a> SlotStopArgs<'a> {
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
    pub clip_id: RtClipId,
    pub supplier_chain: SupplierChain,
    pub project: Option<Project>,
    pub shared_pos: SharedPos,
    pub shared_peak: SharedPeak,
    pub timeline: HybridTimeline,
    pub timeline_cursor_pos: PositionInSeconds,
    pub settings: MatrixClipRecordSettings,
}

#[derive(Debug)]
pub struct MidiOverdubInstruction {
    pub clip_index: usize,
    /// We can't overdub on a file-based MIDI source. If the current MIDI source is a file-based
    /// one, this field will contain a MidiSequence. The current real-time source needs
    /// to be replaced with this one before overdubbing can work.
    pub source_replacement: Option<MidiSequence>,
    pub settings: MidiOverdubSettings,
}

#[derive(Debug)]
pub struct ClipRecordArgs {
    pub recording_equipment: RecordingEquipment,
    pub settings: MatrixClipRecordSettings,
}

#[derive(Debug)]
pub struct ApplyClipArgs<'a> {
    pub other_clip: &'a mut RtClip,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
}

#[derive(Debug)]
pub struct SetClipSettingsArgs<'a> {
    pub clip_settings: RtClipSettings,
    pub matrix_settings: &'a OverridableMatrixSettings,
    pub column_settings: &'a RtColumnSettings,
}

#[derive(Eq, PartialEq, Debug)]
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
    pub column_settings: &'a RtColumnSettings,
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

/// Play state of a slot.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub struct InternalClipPlayState(pub ClipPlayState);

impl From<ClipPlayState> for InternalClipPlayState {
    fn from(inner: ClipPlayState) -> Self {
        Self(inner)
    }
}

impl InternalClipPlayState {
    pub fn get(&self) -> ClipPlayState {
        self.0
    }

    pub fn id_string(&self) -> &'static str {
        use ClipPlayState::*;
        match self.0 {
            Stopped => "stopped",
            ScheduledForPlayStart => "scheduled_for_play_start",
            Playing => "playing",
            Paused => "paused",
            ScheduledForPlayStop => "scheduled_for_play_stop",
            ScheduledForRecordingStart => "scheduled_for_record_start",
            Recording => "recording",
            ScheduledForRecordingStop => "scheduled_for_record_stop",
        }
    }

    /// Translates this play state into a feedback value.
    pub fn feedback_value(self) -> UnitValue {
        use ClipPlayState::*;
        match self.0 {
            Stopped => UnitValue::new(0.1),
            ScheduledForPlayStart => UnitValue::new(0.75),
            Playing => UnitValue::MAX,
            Paused => UnitValue::new(0.5),
            ScheduledForPlayStop => UnitValue::new(0.25),
            Recording => UnitValue::new(0.60),
            ScheduledForRecordingStart => UnitValue::new(0.9),
            ScheduledForRecordingStop => UnitValue::new(0.4),
        }
    }

    /// If you want to know if it's worth to push out position updates.
    ///
    /// Attention: This will return `false` if the clip is being suspended (e.g. fading out), so
    /// don't use this to check whether processing is still necessary.
    pub fn is_advancing(&self) -> bool {
        use ClipPlayState::*;
        matches!(
            self.0,
            ScheduledForPlayStart
                | Playing
                | ScheduledForPlayStop
                | ScheduledForRecordingStart
                | Recording
                | ScheduledForRecordingStop
        )
    }

    pub fn is_somehow_recording(&self) -> bool {
        use ClipPlayState::*;
        matches!(
            self.0,
            ScheduledForRecordingStart | Recording | ScheduledForRecordingStop
        )
    }

    pub fn is_as_good_as_playing(&self) -> bool {
        use ClipPlayState::*;
        matches!(self.0, ScheduledForPlayStart | Playing)
    }

    pub fn is_as_good_as_recording(&self) -> bool {
        use ClipPlayState::*;
        matches!(self.0, ScheduledForRecordingStart | Recording)
    }

    pub fn is_stoppable(&self) -> bool {
        self.is_as_good_as_playing() || self.is_as_good_as_recording()
    }
}

impl Default for InternalClipPlayState {
    fn default() -> Self {
        Self(ClipPlayState::Stopped)
    }
}

#[derive(Debug)]
pub enum SlotChangeEvent {
    PlayState(InternalClipPlayState),
    Clips(&'static str),
    Continuous {
        proportional: UnitValue,
        seconds: PositionInSeconds,
        peak: UnitValue,
    },
}

#[derive(Debug)]
pub struct QualifiedClipChangeEvent {
    pub clip_address: ClipAddress,
    pub event: ClipChangeEvent,
}

#[derive(Debug)]
pub enum ClipChangeEvent {
    /// Everything within the clip has potentially changed.
    Everything,
    // TODO-high-clip-engine Is special handling for volume and looped necessary?
    Volume(Db),
    Looped(bool),
}

#[derive(Debug)]
pub struct QualifiedSlotChangeEvent {
    pub slot_address: ClipSlotAddress,
    pub event: SlotChangeEvent,
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

#[derive(Default)]
pub struct ClipProcessingOutcome {
    pub num_audio_frames_written: usize,
}

struct FillSamplesOutcome {
    num_audio_frames_written: usize,
    next_frame: Option<isize>,
}

pub trait HandleSlotEvent {
    fn midi_overdub_finished(&self, clip_id: RtClipId, outcome: MidiOverdubOutcome);
    fn normal_recording_finished(&self, outcome: NormalRecordingOutcome);
    fn slot_cleared(&self, clips: RtClips);
}

/// Holds the result of a normal (non-overdub) recording.
///
/// Can also be cancelled.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum NormalRecordingOutcome {
    Committed(CommittedRecording),
    Canceled,
}

/// Holds the data of a successful recording (material and settings).
#[derive(Clone, Debug)]
pub struct CommittedRecording {
    pub kind_specific: KindSpecificRecordingOutcome,
    pub clip_settings: RtClipSettings,
    pub material_info: MaterialInfo,
}

/// All settings of a clip that affect processing.
///
/// To be sent back to the main thread to update the main thread clip.
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct RtClipSettings {
    pub time_base: api::ClipTimeBase,
    pub looped: bool,
    pub volume: api::Db,
    pub section: api::Section,
    pub start_timing: Option<api::ClipPlayStartTiming>,
    pub stop_timing: Option<api::ClipPlayStopTiming>,
    pub audio_settings: api::ClipAudioSettings,
    pub midi_settings: api::ClipMidiSettings,
}

impl RtClipSettings {
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
        column_settings: &RtColumnSettings,
    ) -> ClipEngineResult<Self> {
        let current_play_start_timing = column_settings
            .clip_play_start_timing
            .unwrap_or(matrix_settings.clip_play_start_timing);
        let settings = Self {
            start_timing: record_settings.effective_play_start_timing(
                data.initial_play_start_timing,
                current_play_start_timing,
            ),
            stop_timing: record_settings.effective_play_stop_timing(
                data.initial_play_start_timing,
                current_play_start_timing,
            ),
            looped: record_settings.looped,
            time_base: {
                let audio_tempo = if data.is_midi {
                    None
                } else {
                    Some(api::Bpm::new(data.tempo.get())?)
                };
                record_settings.effective_play_time_base(
                    data.initial_play_start_timing,
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
            midi_settings: record_settings.midi_settings.clip_settings,
        };
        Ok(settings)
    }

    fn create_chain_settings(
        &self,
        matrix_settings: &OverridableMatrixSettings,
        column_settings: &RtColumnSettings,
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
    let end_bar_timeline_pos = args
        .timeline
        .pos_of_quantized_pos(QuantizedPosition::bar(end_bar as i64));
    debug_assert!(
        end_bar_timeline_pos > start_bar_timeline_pos,
        "end_bar_timeline_pos {end_bar_timeline_pos} <= start_bar_timeline_pos {start_bar_timeline_pos}",
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
    pub fn from_on_audio_buffer_args(args: &OnAudioBufferArgs) -> Self {
        Self {
            block_length: args.len as _,
            frame_rate: args.srate,
        }
    }

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

impl<'a> QuantizedPosCalcEquipment<'a> {
    pub fn new_with_unmodified_tempo(
        timeline: &'a HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
        timeline_tempo: Bpm,
        audio_request_props: BasicAudioRequestProps,
        is_midi: bool,
    ) -> Self {
        QuantizedPosCalcEquipment {
            audio_request_props,
            timeline,
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
        }
    }
}

fn modulo_frame(frame: isize, frame_count: usize) -> isize {
    if frame < 0 {
        frame
    } else {
        frame % frame_count as isize
    }
}
