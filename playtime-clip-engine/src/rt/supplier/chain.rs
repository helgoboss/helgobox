use crate::mutex_util::non_blocking_lock;
use crate::rt::supplier::{
    Amplifier, AudioSupplier, Cache, CacheRequest, CommandProcessor, Downbeat, InteractionHandler,
    LoopBehavior, Looper, MaterialInfo, MidiSupplier, PollRecordingOutcome,
    PositionTranslationSkill, PreBuffer, PreBufferCacheMissBehavior, PreBufferFillRequest,
    PreBufferOptions, PreBufferRequest, PreBufferSourceSkill, RecordState, Recorder, RecordingArgs,
    Resampler, Section, StartEndHandler, StopRecordingOutcome, SupplyAudioRequest,
    SupplyMidiRequest, SupplyResponse, TimeStretcher, WithMaterialInfo, WriteAudioRequest,
    WriteMidiRequest,
};
use crate::rt::tempo_util::determine_tempo_from_beat_time_base;
use crate::rt::{AudioBufMut, BasicAudioRequestProps};
use crate::{ClipEngineResult, HybridTimeline};
use crossbeam_channel::Sender;
use playtime_api as api;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, ClipTimeBase, Db, MidiResetMessageRange,
    PositiveBeat, PositiveSecond, VirtualResampleMode,
};
use reaper_medium::{BorrowedMidiEventList, Bpm, OwnedPcmSource, PositionInSeconds};
use std::sync::{Arc, Mutex, MutexGuard};

/// The head of the supplier chain (just an alias).
type Head = AmplifierTail;

/// Responsible for changing the volume.
///
/// It sits on top of everything because volume changes are fast and shouldn't be cached because
/// they can happen very suddenly (e.g. in response to different velocity values).
type AmplifierTail = Amplifier<ResamplerTail>;

/// Resampler takes care of converting between the requested destination (= output) frame rate
/// and the frame rate of the inner material. It's also responsible for changing the tempo of MIDI
/// material and optionally even audio material (VariSpeed = not preserving pitch).
///
/// We have the resampler on top of the interaction handler because at the moment the interaction
/// handler logic is based on the assumption that input frame rate == output frame rate. If we want
/// to put the interaction handler above the resampler one day (e.g. for caching reasons), we first
/// must change the logic accordingly (doing some frame rate conversions).
///
/// At the moment, I think resampling results don't need to be pre-buffered. However, as soon as
/// we decide that they do, the interaction handler should definitely move above the resampler.
type ResamplerTail = Resampler<InteractionHandlerTail>;

/// Interaction handler handles sudden interactions, introducing proper fades and reset messages.
///
/// It sits on top of almost everything because it's fast and shouldn't be cached (because
/// interactions are by definition very sudden events).
type InteractionHandlerTail = InteractionHandler<TimeStretcherTail>;

/// Time stretcher is responsible for stretching audio material while preserving its pitch.
///
/// It sits on top of the (downbeat-shifted) looper (not deeper) because it greedily grabs
/// material from its supplier - a kind of internal look-ahead/pre-buffering. If it would reach the
/// end of material and we want to start the next loop cycle, it wouldn't have material ready and
/// would need to start pre-buffering from scratch. Not good.
///
/// It sits above the pre-buffer at the moment although time-stretching results also should be
/// pre-buffered (because time stretching is slow). Pre-buffering time-stretching results probably
/// has some special needs that we don't handle yet. Let's see.
type TimeStretcherTail = TimeStretcher<DownbeatTail>;

/// Downbeat handler sits on top of the looper because it moves the complete loop to the left
/// (it helps imagining the material as items in the arrangement view).
///
/// It even sits on top of the pre-buffer because it just moves the material, which is easy to
/// express by cheaply converting pre-buffer requests. In general, we don't want to pre-buffer
/// more non-destructive changes than necessary, especially not the sudden changes (because that
/// would introduce a latency).
///
/// Also, the pre-buffer can apply an optimization if it can be sure that there's no material
/// in the count-in phase, which is true for all material below the downbeat handler.
type DownbeatTail = Downbeat<PreBufferTail>;

/// Pre-buffer asynchronously loads a small amount of audio source material into memory before it's
/// being played. That's important because the inner-most source usually reads audio material
/// directly from disk and disk access can be slow.
///
/// It sits above the looper (not just above the inner-most source), because it needs to grab
/// material in advance. The looper knows best which material comes next. If it would sit below
/// the looper and it would reach end of material, it doesn't have anything in hand to decide what
/// needs to be pre-buffered next.
type PreBufferTail =
    PreBuffer<SharedLooperTail, ChainPreBufferCommandProcessor, ChainPreBufferCommand>;

/// Everything below the pre-buffer is shared because we must access it from the pre-buffer worker
/// thread as well. We make sure of having no contention when locking the mutex.
type SharedLooperTail = Arc<Mutex<LooperTail>>;

/// Looper optionally repeats the material.
///
/// It sits above the section because the section needs to be looped, not the full source.
type LooperTail = Looper<SectionTail>;

/// Section handler optionally plays just a certain portion of the material. It can also be used to
/// add silence after end of material.
///
/// It sits above the start-end handler because it has its own non-optional section
/// start-end handling. It could probably also sit below the start-end handler and the start-end
/// handler could be configured to handle section start-end as well, but at the moment it's fine as
/// it is.
type SectionTail = Section<StartEndHandlerTail>;

/// Start-end handler introduces fades and reset messages to "fix" the source material and make it
/// ready for being looped.
///
/// It sits on top of the recorder (representing the inner-most source) because it's
/// optional and intended to really affect only the inner-most source, not the section or loop
/// (which have their own start-end handling).
type StartEndHandlerTail = StartEndHandler<CacheTail>;

/// Cache handler optionally caches the complete original source material in memory.
///
/// It sits on top of recorder so that recorder doesn't have to deal with swapping caches (it has
/// to do enough already).
///
/// It sits below the other suppliers because if we cache a big chunk in memory, we want to be sure
/// we can reuse it in lots of different ways.
type CacheTail = Cache<RecorderTail>;

/// Recorder takes care of recording and swapping sources.
///
/// When it comes to playing (not recording), it basically represents the source = the inner-most
/// material.
///
/// It's hard-coded to sit on top of `OwnedPcmSource` because it's responsible for swapping an old
/// source with a newly recorded source.
type RecorderTail = Recorder;

#[derive(Debug)]
pub struct SupplierChain {
    head: Head,
}

impl SupplierChain {
    pub fn new(recorder: Recorder, equipment: ChainEquipment) -> ClipEngineResult<Self> {
        let pre_buffer_options = PreBufferOptions {
            // We know we sit below the downbeat handler, so the underlying suppliers won't deliver
            // material in the count-in phase.
            skip_count_in_phase_material: true,
            cache_miss_behavior: PreBufferCacheMissBehavior::OutputSilence,
            recalibrate_on_cache_miss: false,
        };
        let mut looper = Looper::new(Section::new(StartEndHandler::new(Cache::new(
            recorder,
            equipment.cache_request_sender,
        ))));
        looper.set_enabled(true);
        let mut chain = Self {
            head: {
                Amplifier::new(Resampler::new(InteractionHandler::new(TimeStretcher::new(
                    Downbeat::new(PreBuffer::new(
                        Arc::new(Mutex::new(looper)),
                        equipment.pre_buffer_request_sender,
                        pre_buffer_options,
                        ChainPreBufferCommandProcessor,
                    )),
                ))))
            },
        };
        // Configure resampler
        let resampler = chain.resampler_mut();
        resampler.set_enabled(true);
        // Configure time stretcher
        let time_stretcher = chain.time_stretcher_mut();
        time_stretcher.set_enabled(true);
        // Configure downbeat
        let downbeat = chain.downbeat_mut();
        downbeat.set_enabled(true);
        // Configure pre-buffer
        let pre_buffer = chain.pre_buffer_mut();
        let _ = pre_buffer.enable();
        Ok(chain)
    }

    /// At the moment not suitable for applying while playing (because no special handling for
    /// looped).
    pub fn configure_complete_chain(&mut self, settings: ChainSettings) -> ClipEngineResult<()> {
        let material_info = self.material_info()?;
        self.set_looped(settings.looped);
        self.set_time_base(&settings.time_base, material_info.is_midi())?;
        self.set_volume(settings.volume);
        self.set_section_bounds_in_seconds(settings.section.start_pos, settings.section.length);
        self.set_audio_fades_enabled_for_source(settings.audio_apply_source_fades);
        self.set_audio_time_stretch_mode(settings.audio_time_stretch_mode);
        self.set_audio_resample_mode(settings.audio_resample_mode);
        self.set_audio_cache_behavior(settings.cache_behavior);
        self.set_midi_settings(settings.midi_settings);
        Ok(())
    }

    pub fn pre_buffer_simple(&mut self, next_expected_pos: isize) {
        if self.material_info().map(|i| i.is_midi()).unwrap_or(false) {
            // MIDI doesn't need pre-buffering
            return;
        }
        let req = PreBufferFillRequest {
            start_frame: next_expected_pos,
        };
        self.pre_buffer(req);
    }

    fn set_time_base(&mut self, time_base: &ClipTimeBase, is_midi: bool) -> ClipEngineResult<()> {
        match time_base {
            ClipTimeBase::Time => {
                self.set_time_stretching_enabled(false);
                self.clear_downbeat();
            }
            ClipTimeBase::Beat(b) => {
                self.set_time_stretching_enabled(true);
                let tempo = determine_tempo_from_beat_time_base(b, is_midi);
                self.set_downbeat_in_beats(b.downbeat, tempo)?;
            }
        }
        Ok(())
    }

    pub fn is_playing_already(&self, pos: isize) -> bool {
        let downbeat_correct_pos = pos + self.downbeat().downbeat_frame() as isize;
        downbeat_correct_pos >= 0
    }

    fn clear_downbeat(&mut self) {
        self.downbeat_mut().set_downbeat_frame(0);
    }

    fn set_audio_fades_enabled_for_source(&mut self, enabled: bool) {
        let command = ChainPreBufferCommand::SetAudioFadesEnabledForSource(enabled);
        self.pre_buffer_supplier().send_command(command);
    }

    fn set_midi_settings(&mut self, settings: api::ClipMidiSettings) {
        self.set_midi_reset_msg_range_for_interaction(settings.interaction_reset_settings);
        self.set_midi_reset_msg_range_for_source(settings.source_reset_settings);
        self.set_midi_reset_msg_range_for_section(settings.section_reset_settings);
        self.set_midi_reset_msg_range_for_loop(settings.loop_reset_settings);
    }

    fn set_midi_reset_msg_range_for_section(&mut self, range: MidiResetMessageRange) {
        let command = ChainPreBufferCommand::SetMidiResetMsgRangeForSection(range);
        self.pre_buffer_supplier().send_command(command);
    }

    fn set_midi_reset_msg_range_for_interaction(&mut self, range: MidiResetMessageRange) {
        self.interaction_handler_mut()
            .set_midi_reset_msg_range(range);
    }

    fn set_midi_reset_msg_range_for_loop(&mut self, range: MidiResetMessageRange) {
        let command = ChainPreBufferCommand::SetMidiResetMsgRangeForLoop(range);
        self.pre_buffer_supplier().send_command(command);
    }

    fn set_midi_reset_msg_range_for_source(&mut self, range: MidiResetMessageRange) {
        let command = ChainPreBufferCommand::SetMidiResetMsgRangeForSource(range);
        self.pre_buffer_supplier().send_command(command);
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.amplifier_mut()
            .set_volume(reaper_medium::Db::new(volume.get()));
    }

    fn set_downbeat_in_beats(&mut self, beat: PositiveBeat, tempo: Bpm) -> ClipEngineResult<()> {
        self.downbeat_mut().set_downbeat_in_beats(beat, tempo)
    }

    fn set_audio_resample_mode(&mut self, mode: VirtualResampleMode) {
        self.resampler_mut().set_mode(mode);
    }

    pub fn register_midi_overdub_mirror_source(&mut self, mirror_source: OwnedPcmSource) {
        // With MIDI, there's no contention.
        self.pre_buffer_wormhole()
            .recorder()
            .register_midi_overdub_mirror_source(mirror_source)
            .unwrap();
    }

    pub fn take_midi_overdub_mirror_source(&mut self) -> Option<OwnedPcmSource> {
        // With MIDI, there's no contention.
        self.pre_buffer_wormhole()
            .recorder()
            .take_midi_overdub_mirror_source()
    }

    /// If we are in MIDI overdub mode, the play position parameter must be set.
    pub fn write_midi(
        &mut self,
        request: WriteMidiRequest,
        play_pos: Option<isize>,
    ) -> ClipEngineResult<()> {
        // When recording, there's no contention.
        let translated_play_pos = match play_pos {
            None => None,
            Some(play_pos) => {
                let translated = self.translate_play_pos_to_source_pos(play_pos);
                if translated < 0 {
                    return Err("translated play position is not within source bounds");
                }
                Some(translated as usize)
            }
        };
        self.pre_buffer_wormhole()
            .recorder()
            .write_midi(request, translated_play_pos)
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        // When recording, there's no contention.
        self.pre_buffer_wormhole()
            .recorder()
            .write_audio(request)
            .unwrap();
    }

    pub fn record_state(&self) -> Option<RecordState> {
        self.pre_buffer_wormhole().recorder().record_state()
    }

    pub fn poll_recording(
        &mut self,
        audio_request_props: BasicAudioRequestProps,
    ) -> PollRecordingOutcome {
        self.pre_buffer_wormhole()
            .recorder()
            .poll_recording(audio_request_props)
    }

    pub fn stop_recording(
        &mut self,
        timeline: &HybridTimeline,
        timeline_cursor_pos: PositionInSeconds,
        audio_request_props: BasicAudioRequestProps,
    ) -> ClipEngineResult<StopRecordingOutcome> {
        self.pre_buffer_wormhole().recorder().stop_recording(
            timeline,
            timeline_cursor_pos,
            audio_request_props,
        )
    }

    pub fn prepare_recording(&mut self, args: RecordingArgs) {
        // When recording, there's no contention.
        self.pre_buffer_wormhole()
            .recorder()
            .prepare_recording(args)
            .unwrap();
    }

    fn set_audio_cache_behavior(&mut self, cache_behavior: AudioCacheBehavior) {
        use AudioCacheBehavior::*;
        let pre_buffer_enabled = match &cache_behavior {
            DirectFromDisk => true,
            CacheInMemory => false,
        };
        let command = ChainPreBufferCommand::SetAudioCacheBehavior(cache_behavior);
        self.pre_buffer_supplier().send_command(command);
        // Enable/disable pre-buffer accordingly (pre-buffering not necessary if we have the
        // complete source material in memory already).
        let pre_buffer = self.pre_buffer_mut();
        if pre_buffer_enabled {
            let _ = pre_buffer.enable();
        } else {
            pre_buffer.disable();
        }
    }

    fn set_audio_time_stretch_mode(&mut self, mode: AudioTimeStretchMode) {
        use AudioTimeStretchMode::*;
        let use_vari_speed = match mode {
            VariSpeed => true,
            KeepingPitch(m) => {
                self.time_stretcher_mut().set_mode(m.mode);
                false
            }
        };
        self.resampler_mut()
            .set_responsible_for_audio_time_stretching(use_vari_speed);
        self.time_stretcher_mut()
            .set_responsible_for_audio_time_stretching(!use_vari_speed);
    }

    fn set_time_stretching_enabled(&mut self, enabled: bool) {
        self.time_stretcher_mut().set_enabled(enabled);
    }

    pub fn set_looped(&mut self, looped: bool) {
        let command = ChainPreBufferCommand::SetLooped(looped);
        self.pre_buffer_supplier().send_command(command);
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.resampler_mut().set_tempo_factor(tempo_factor);
        self.time_stretcher_mut().set_tempo_factor(tempo_factor);
    }

    pub fn install_immediate_start_interaction(&mut self, current_frame: isize) {
        self.interaction_handler_mut()
            .start_immediately(current_frame)
            .unwrap();
    }

    pub fn stop_interaction_is_installed_already(&self) -> bool {
        self.interaction_handler().has_stop_interaction()
    }

    pub fn install_immediate_stop_interaction(&mut self, current_frame: isize) {
        self.interaction_handler_mut()
            .stop_immediately(current_frame)
            .unwrap();
    }

    pub fn schedule_stop_interaction_at(&mut self, frame: isize) {
        self.interaction_handler_mut().schedule_stop_at(frame);
    }

    pub fn reset_interactions(&mut self) {
        self.interaction_handler_mut().reset();
    }

    pub fn reset_for_play(&mut self, looped: bool) {
        self.interaction_handler_mut().reset();
        self.resampler_mut().reset_buffers_and_latency();
        self.time_stretcher_mut().reset_buffers_and_latency();
        self.set_looped(looped);
    }

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) {
        let command = ChainPreBufferCommand::KeepPlayingUntilEndOfCurrentCycle { pos };
        self.pre_buffer_supplier().send_command(command);
    }

    fn set_section_bounds_in_seconds(
        &mut self,
        start: PositiveSecond,
        length: Option<PositiveSecond>,
    ) {
        let command = ChainPreBufferCommand::SetSectionBoundsInSeconds { start, length };
        self.pre_buffer_supplier().send_command(command);
    }

    fn amplifier(&self) -> &AmplifierTail {
        &self.head
    }

    fn amplifier_mut(&mut self) -> &mut AmplifierTail {
        &mut self.head
    }

    fn interaction_handler(&self) -> &InteractionHandlerTail {
        self.resampler().supplier()
    }

    fn interaction_handler_mut(&mut self) -> &mut InteractionHandlerTail {
        self.resampler_mut().supplier_mut()
    }

    fn resampler(&self) -> &ResamplerTail {
        self.amplifier().supplier()
    }

    fn resampler_mut(&mut self) -> &mut ResamplerTail {
        self.amplifier_mut().supplier_mut()
    }

    fn time_stretcher(&self) -> &TimeStretcherTail {
        self.interaction_handler().supplier()
    }

    fn time_stretcher_mut(&mut self) -> &mut TimeStretcherTail {
        self.interaction_handler_mut().supplier_mut()
    }

    fn downbeat(&self) -> &DownbeatTail {
        self.time_stretcher().supplier()
    }

    fn downbeat_mut(&mut self) -> &mut DownbeatTail {
        self.time_stretcher_mut().supplier_mut()
    }

    fn pre_buffer_supplier(&self) -> &PreBufferTail {
        self.downbeat().supplier()
    }

    fn pre_buffer_mut(&mut self) -> &mut PreBufferTail {
        self.downbeat_mut().supplier_mut()
    }

    /// Allows accessing the suppliers below the pre-buffer.
    ///
    /// Attention: This attempts to lock a mutex and panics if it's locked already. Therefore it can
    /// be used only if one is sure that there can't be any contention!
    ///
    /// # Panics
    ///
    /// This method panics if the mutex is locked!
    fn pre_buffer_wormhole(&self) -> MutexGuard<LooperTail> {
        non_blocking_lock(
            self.pre_buffer_supplier().supplier(),
            "attempt to access pre-buffer wormhole from chain while locked",
        )
    }
}

trait Entrance {
    fn looper(&mut self) -> &mut LooperTail;

    fn section(&mut self) -> &mut SectionTail;

    fn start_end_handler(&mut self) -> &mut StartEndHandlerTail;

    fn cache(&mut self) -> &mut CacheTail;

    fn recorder(&mut self) -> &mut RecorderTail;
}

impl<'a> Entrance for MutexGuard<'a, LooperTail> {
    fn looper(&mut self) -> &mut LooperTail {
        self
    }

    fn section(&mut self) -> &mut SectionTail {
        self.supplier_mut()
    }

    fn start_end_handler(&mut self) -> &mut StartEndHandlerTail {
        self.section().supplier_mut()
    }

    fn cache(&mut self) -> &mut CacheTail {
        self.start_end_handler().supplier_mut()
    }

    fn recorder(&mut self) -> &mut RecorderTail {
        self.cache().supplier_mut()
    }
}

impl AudioSupplier for SupplierChain {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.head.supply_audio(request, dest_buffer)
    }
}

impl MidiSupplier for SupplierChain {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        self.head.supply_midi(request, event_list)
    }
}

impl WithMaterialInfo for SupplierChain {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.head.material_info()
    }
}

impl PreBufferSourceSkill for SupplierChain {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.head.pre_buffer(request)
    }
}

impl PositionTranslationSkill for SupplierChain {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        self.head.translate_play_pos_to_source_pos(play_pos)
    }
}

pub type ChainPreBufferRequest = PreBufferRequest<SharedLooperTail, ChainPreBufferCommand>;

#[derive(Debug)]
pub enum ChainPreBufferCommand {
    SetAudioFadesEnabledForSource(bool),
    SetMidiResetMsgRangeForSection(MidiResetMessageRange),
    SetMidiResetMsgRangeForLoop(MidiResetMessageRange),
    SetMidiResetMsgRangeForSource(MidiResetMessageRange),
    SetAudioCacheBehavior(AudioCacheBehavior),
    SetLooped(bool),
    KeepPlayingUntilEndOfCurrentCycle {
        pos: isize,
    },
    SetSectionBoundsInFrames {
        start: usize,
        length: Option<usize>,
    },
    SetSectionBoundsInSeconds {
        start: PositiveSecond,
        length: Option<PositiveSecond>,
    },
}

#[derive(Debug)]
pub struct ChainPreBufferCommandProcessor;

impl CommandProcessor for ChainPreBufferCommandProcessor {
    type Supplier = SharedLooperTail;
    type Command = ChainPreBufferCommand;

    fn process_command(&self, command: ChainPreBufferCommand, supplier: &SharedLooperTail) {
        let mut entrance = non_blocking_lock(&*supplier, "command processing");
        use ChainPreBufferCommand::*;
        match command {
            SetAudioFadesEnabledForSource(enabled) => {
                entrance
                    .start_end_handler()
                    .set_audio_fades_enabled(enabled);
            }
            SetMidiResetMsgRangeForSection(range) => {
                entrance.section().set_midi_reset_msg_range(range);
            }
            SetMidiResetMsgRangeForLoop(range) => {
                entrance.looper().set_midi_reset_msg_range(range);
            }
            SetMidiResetMsgRangeForSource(range) => {
                entrance.start_end_handler().set_midi_reset_msg_range(range);
            }
            SetAudioCacheBehavior(behavior) => {
                entrance.cache().set_audio_cache_behavior(behavior);
            }
            SetLooped(looped) => entrance
                .looper()
                .set_loop_behavior(LoopBehavior::from_bool(looped)),
            KeepPlayingUntilEndOfCurrentCycle { pos } => {
                entrance
                    .looper()
                    .keep_playing_until_end_of_current_cycle(pos)
                    .unwrap();
            }
            SetSectionBoundsInFrames { start, length } => {
                entrance.section().set_bounds(start, length);
                configure_start_end_handler_on_section_change(
                    entrance.start_end_handler(),
                    start > 0,
                    length.is_some(),
                );
            }
            SetSectionBoundsInSeconds { start, length } => {
                entrance
                    .section()
                    .set_bounds_in_seconds(start, length)
                    .unwrap();
                configure_start_end_handler_on_section_change(
                    entrance.start_end_handler(),
                    start.get() > 0.0,
                    length.is_some(),
                );
            }
        }
    }
}

fn configure_start_end_handler_on_section_change(
    start_end_handler: &mut StartEndHandlerTail,
    start_is_set: bool,
    length_is_set: bool,
) {
    // Let the section handle the start/end fades etc. if appropriate (in order to not have
    // unnecessary fades).
    start_end_handler.set_enabled_for_start(!start_is_set);
    start_end_handler.set_enabled_for_end(!length_is_set);
}

#[derive(Clone, Debug)]
pub struct ChainEquipment {
    pub pre_buffer_request_sender: Sender<ChainPreBufferRequest>,
    pub cache_request_sender: Sender<CacheRequest>,
}
/// Everything necessary to configure the clip supply chain.
#[derive(Copy, Clone, Debug)]
pub struct ChainSettings {
    pub time_base: api::ClipTimeBase,
    pub midi_settings: api::ClipMidiSettings,
    pub looped: bool,
    pub volume: api::Db,
    pub section: api::Section,
    pub audio_apply_source_fades: bool,
    pub audio_time_stretch_mode: AudioTimeStretchMode,
    pub audio_resample_mode: VirtualResampleMode,
    pub cache_behavior: AudioCacheBehavior,
}
