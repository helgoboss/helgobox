use crate::rt::supplier::{
    Amplifier, AudioSupplier, Downbeat, InteractionHandler, LoopBehavior, Looper, MaterialInfo,
    MidiSupplier, PreBuffer, PreBufferFillRequest, PreBufferRequest, PreBufferSourceSkill,
    Recorder, RecordingOutcome, Resampler, Section, StartEndHandler, SupplyAudioRequest,
    SupplyMidiRequest, SupplyResponse, TimeStretcher, WithMaterialInfo, WriteAudioRequest,
    WriteMidiRequest,
};
use crate::rt::{AudioBufMut, ClipRecordInput, RecordTiming};
use crate::{ClipEngineResult, Timeline};
use crossbeam_channel::Sender;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, Db, MidiResetMessageRange, PositiveBeat,
    PositiveSecond, VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{BorrowedMidiEventList, Bpm, DurationInSeconds, Hz, PositionInSeconds};
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
type DownbeatTail = Downbeat<PreBufferTail>;

/// Pre-buffer asynchronously loads a small amount of audio source material into memory before it's
/// being played. That's important because the inner-most source usually reads audio material
/// directly from disk and disk access can be slow.
///
/// It sits above the looper (not just above the inner-most source), because it needs to grab
/// material in advance. The looper knows best which material comes next. If it would sit below
/// the looper and it would reach end of material, it doesn't have anything in hand to decide what
/// needs to be pre-buffered next.
type PreBufferTail = PreBuffer<Arc<Mutex<LooperTail>>>;

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
type StartEndHandlerTail = StartEndHandler<RecorderTail>;

/// Recorder takes care of recording and swapping sources.
///
/// When it comes to playing (not recording), it basically represents the source = the inner-most
/// material.
///
/// It's hard-coded to sit on top of `Cache<OwnedPcmSource>` because it's responsible
/// for swapping an old source with a newly recorded source.
type RecorderTail = Recorder;

#[derive(Debug)]
pub struct SupplierChain {
    head: Head,
}

impl SupplierChain {
    pub fn new(recorder: Recorder, pre_buffer_request_sender: Sender<PreBufferRequest>) -> Self {
        let mut chain = Self {
            head: {
                Amplifier::new(Resampler::new(InteractionHandler::new(TimeStretcher::new(
                    Downbeat::new(PreBuffer::new(
                        Arc::new(Mutex::new(Looper::new(Section::new(StartEndHandler::new(
                            recorder,
                        ))))),
                        pre_buffer_request_sender,
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
        pre_buffer.set_enabled(true);
        // Configure looper
        // TODO-high-prebuffer OK, fire and forget
        chain.entrance().looper().set_enabled(true);
        chain
    }

    pub fn is_playing_already(&self, pos: isize) -> bool {
        let downbeat_correct_pos = pos + self.downbeat().downbeat_frame() as isize;
        downbeat_correct_pos >= 0
    }

    pub fn clear_downbeat(&mut self) {
        self.downbeat_mut().set_downbeat_frame(0);
    }

    pub fn set_audio_fades_enabled_for_source(&mut self, enabled: bool) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .start_end_handler()
            .set_audio_fades_enabled(enabled);
    }

    pub fn set_midi_reset_msg_range_for_section(&mut self, range: MidiResetMessageRange) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance().section().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_interaction(&mut self, range: MidiResetMessageRange) {
        self.interaction_handler_mut()
            .set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_loop(&mut self, range: MidiResetMessageRange) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance().looper().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_source(&mut self, range: MidiResetMessageRange) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .start_end_handler()
            .set_midi_reset_msg_range(range);
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.amplifier_mut()
            .set_volume(reaper_medium::Db::new(volume.get()));
    }

    pub fn set_section_bounds_in_seconds(
        &mut self,
        start: PositiveSecond,
        length: Option<PositiveSecond>,
    ) -> ClipEngineResult<()> {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .section()
            .set_bounds_in_seconds(start, length)
    }

    pub fn set_downbeat_in_beats(
        &mut self,
        beat: PositiveBeat,
        tempo: Bpm,
    ) -> ClipEngineResult<()> {
        self.downbeat_mut().set_downbeat_in_beats(beat, tempo)
    }

    pub fn set_downbeat_in_frames(&mut self, frame: usize) {
        self.downbeat_mut().set_downbeat_frame(frame);
    }

    pub fn set_audio_resample_mode(&mut self, mode: VirtualResampleMode) {
        self.resampler_mut().set_mode(mode);
    }

    pub fn schedule_end_of_recording(&mut self, end_bar: i32, timeline: &dyn Timeline) {
        todo!()
    }

    pub fn write_midi(&mut self, request: WriteMidiRequest, pos: DurationInSeconds) {
        todo!()
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        todo!()
    }

    pub fn rollback_recording(&mut self) -> ClipEngineResult<()> {
        todo!()
    }

    pub fn commit_recording(
        &mut self,
        timeline: &dyn Timeline,
    ) -> ClipEngineResult<RecordingOutcome> {
        todo!()
    }

    /// This must not be done in a real-time thread!
    pub fn prepare_recording(
        &mut self,
        input: ClipRecordInput,
        project: Option<Project>,
        trigger_timeline_pos: PositionInSeconds,
        tempo: Bpm,
        detect_downbeat: bool,
        timing: RecordTiming,
    ) {
        todo!()
    }

    pub fn set_audio_cache_behavior(
        &mut self,
        cache_behavior: AudioCacheBehavior,
    ) -> ClipEngineResult<()> {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .recorder()
            .set_audio_cache_behavior(cache_behavior)
    }

    pub fn set_audio_time_stretch_mode(&mut self, mode: AudioTimeStretchMode) {
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

    pub fn set_time_stretching_enabled(&mut self, enabled: bool) {
        self.time_stretcher_mut().set_enabled(enabled);
    }

    pub fn set_looped(&mut self, looped: bool) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .looper()
            .set_loop_behavior(LoopBehavior::from_bool(looped));
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.resampler_mut().set_tempo_factor(tempo_factor);
        self.time_stretcher_mut().set_tempo_factor(tempo_factor);
    }

    pub fn install_immediate_start_interaction(&mut self, current_frame: isize) {
        self.interaction_handler_mut()
            .start_immediately(current_frame);
    }

    pub fn stop_interaction_is_installed_already(&self) -> bool {
        self.interaction_handler().has_stop_interaction()
    }

    pub fn install_immediate_stop_interaction(&mut self, current_frame: isize) {
        self.interaction_handler_mut()
            .stop_immediately(current_frame);
    }

    pub fn schedule_stop_interaction_at(&mut self, frame: isize) {
        self.interaction_handler_mut().schedule_stop_at(frame);
    }

    pub fn reset_interactions(&mut self) {
        self.interaction_handler_mut().reset();
    }

    pub fn prepare_supply(&mut self) {
        // TODO-high-prebuffer OK, must be integrated into async processing function
        // TODO-high improve guard ... pass by reference?
        let (enabled_for_start, enabled_for_end) = {
            let mut entrance = self.entrance();
            let section = entrance.section();
            // If section start is > 0, the section will take care of applying start fades.
            let enabled_for_start = section.start_frame() == 0;
            // If section end is set, the section will take care of applying end fades.
            let enabled_for_end = section.length().is_none();
            (enabled_for_start, enabled_for_end)
        };
        // TODO-high-prebuffer OK, fire and forget
        let mut entrance = self.entrance();
        let mut start_end_handler = entrance.start_end_handler();
        start_end_handler.set_enabled_for_start(enabled_for_start);
        start_end_handler.set_enabled_for_end(enabled_for_end);
    }

    pub fn reset_for_play(&mut self, looped: bool) {
        self.interaction_handler_mut().reset();
        self.resampler_mut().reset_buffers_and_latency();
        self.time_stretcher_mut().reset_buffers_and_latency();
        // TODO-high-prebuffer OK, fire and forget
        self.entrance()
            .looper()
            .set_loop_behavior(LoopBehavior::from_bool(looped));
    }

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) {
        // TODO-high-prebuffer OK, fire and forget
        let _ = self
            .entrance()
            .looper()
            .keep_playing_until_end_of_current_cycle(pos);
    }

    pub fn set_section_bounds(&mut self, start_frame: usize, length: Option<usize>) {
        // TODO-high-prebuffer OK, fire and forget
        self.entrance().section().set_bounds(start_frame, length);
    }

    pub fn downbeat_pos_during_recording(&self, timeline: &dyn Timeline) -> DurationInSeconds {
        // TODO-high-prebuffer OK, can be cached
        self.entrance()
            .recorder()
            .downbeat_pos_during_recording(timeline)
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

    fn pre_buffer(&self) -> &PreBufferTail {
        self.downbeat().supplier()
    }

    fn pre_buffer_mut(&mut self) -> &mut PreBufferTail {
        self.downbeat_mut().supplier_mut()
    }

    fn entrance(&self) -> MutexGuard<LooperTail> {
        self.pre_buffer().supplier().lock().unwrap()
    }
}

trait Entrance {
    fn looper(&mut self) -> &mut LooperTail;

    fn section(&mut self) -> &mut SectionTail;

    fn start_end_handler(&mut self) -> &mut StartEndHandlerTail;

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

    fn recorder(&mut self) -> &mut RecorderTail {
        self.start_end_handler().supplier_mut()
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
