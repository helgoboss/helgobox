use crate::conversion_util::convert_duration_in_seconds_to_frames;
use crate::rt::supplier::{
    Amplifier, AudioSupplier, Downbeat, ExactDuration, ExactFrameCount, InteractionHandler,
    LoopBehavior, Looper, MidiSupplier, PreBufferFillRequest, PreBufferSourceSkill, Recorder,
    Resampler, Section, StartEndHandler, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    TimeStretcher, WithFrameRate,
};
use crate::rt::AudioBufMut;
use crate::{ClipEngineResult, Timeline};
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, Db, MidiResetMessageRange, PositiveBeat,
    PositiveSecond, VirtualResampleMode,
};
use reaper_medium::{BorrowedMidiEventList, Bpm, DurationInSeconds, Hz};

type Head = AmplifierTail;
type AmplifierTail = Amplifier<InteractionHandlerTail>;
type InteractionHandlerTail = InteractionHandler<ResamplerTail>;
type ResamplerTail = Resampler<TimeStretcherTail>;
type TimeStretcherTail = TimeStretcher<DownbeatTail>;
type DownbeatTail = Downbeat<LooperTail>;
type LooperTail = Looper<SectionTail>;
type SectionTail = Section<StartEndHandlerTail>;
type StartEndHandlerTail = StartEndHandler<RecorderTail>;
// Recorder is hard-coded to sit on top of Cache<PreBuffer<OwnedPcmSource>>.
type RecorderTail = Recorder;

#[derive(Debug)]
pub struct SupplierChain {
    head: Head,
}

impl SupplierChain {
    pub fn new(recorder: Recorder) -> Self {
        let mut chain = Self {
            head: {
                Amplifier::new(InteractionHandler::new(Resampler::new(TimeStretcher::new(
                    Downbeat::new(Looper::new(Section::new(StartEndHandler::new(recorder)))),
                ))))
            },
        };
        // Configure resampler
        let resampler = chain.resampler_mut();
        resampler.set_enabled(true);
        // Configure time stratcher
        let time_stretcher = chain.time_stretcher_mut();
        time_stretcher.set_enabled(true);
        // Configure downbeat
        let downbeat = chain.downbeat_mut();
        downbeat.set_enabled(true);
        // Configure looper
        let looper = chain.looper_mut();
        looper.set_enabled(true);
        // Configure recorder
        let recorder = chain.recorder_mut();
        recorder.set_pre_buffering_enabled(true).unwrap();
        chain
    }

    pub fn is_midi(&self) -> bool {
        self.recorder().is_midi()
    }

    pub fn is_playing_already(&self, pos: isize) -> bool {
        let downbeat_correct_pos = pos + self.downbeat().downbeat_frame() as isize;
        downbeat_correct_pos >= 0
    }

    pub fn clear_downbeat(&mut self) {
        self.downbeat_mut().set_downbeat_frame(0);
    }

    pub fn set_audio_fades_enabled_for_source(&mut self, enabled: bool) {
        self.start_end_handler_mut()
            .set_audio_fades_enabled(enabled);
    }

    pub fn set_midi_reset_msg_range_for_section(&mut self, range: MidiResetMessageRange) {
        self.section_mut().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_interaction(&mut self, range: MidiResetMessageRange) {
        self.interaction_handler_mut()
            .set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_loop(&mut self, range: MidiResetMessageRange) {
        self.looper_mut().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_source(&mut self, range: MidiResetMessageRange) {
        self.start_end_handler_mut().set_midi_reset_msg_range(range);
    }

    pub fn set_volume(&mut self, volume: Db) {
        self.amplifier_mut()
            .set_volume(reaper_medium::Db::new(volume.get()));
    }

    pub fn set_section_in_seconds(
        &mut self,
        start: PositiveSecond,
        length: Option<PositiveSecond>,
    ) -> ClipEngineResult<()> {
        let source_frame_frate = self
            .section()
            .frame_rate()
            .ok_or("can't calculate section frame at the moment because no source available")?;
        let start_frame = convert_duration_in_seconds_to_frames(
            DurationInSeconds::new(start.get()),
            source_frame_frate,
        );
        let frame_count = length.map(|l| {
            convert_duration_in_seconds_to_frames(
                DurationInSeconds::new(l.get()),
                source_frame_frate,
            )
        });
        self.section_mut().set_bounds(start_frame, frame_count);
        Ok(())
    }

    pub fn set_downbeat_in_beats(
        &mut self,
        beat: PositiveBeat,
        tempo: Bpm,
    ) -> ClipEngineResult<()> {
        let source_frame_frate = self
            .downbeat()
            .frame_rate()
            .ok_or("can't calculate downbeat frame at the moment because no source available")?;
        let bps = tempo.get() / 60.0;
        let second = beat.get() / bps;
        let frame = convert_duration_in_seconds_to_frames(
            DurationInSeconds::new(second),
            source_frame_frate,
        );
        self.downbeat_mut().set_downbeat_frame(frame);
        Ok(())
    }

    pub fn set_downbeat_in_frames(&mut self, frame: usize) {
        self.downbeat_mut().set_downbeat_frame(frame);
    }

    pub fn set_audio_resample_mode(&mut self, mode: VirtualResampleMode) {
        self.resampler_mut().set_mode(mode);
    }

    pub fn set_audio_cache_behavior(
        &mut self,
        cache_behavior: AudioCacheBehavior,
    ) -> ClipEngineResult<()> {
        self.recorder_mut().set_audio_cache_behavior(cache_behavior)
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
        self.looper_mut()
            .set_loop_behavior(LoopBehavior::from_bool(looped));
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.resampler_mut().set_tempo_factor(tempo_factor);
        self.time_stretcher_mut().set_tempo_factor(tempo_factor);
    }

    pub fn install_immediate_start_interaction(&mut self, current_frame: isize) {
        let is_midi = self.is_midi();
        self.interaction_handler_mut()
            .start_immediately(current_frame, is_midi);
    }

    pub fn stop_interaction_is_installed_already(&self) -> bool {
        self.interaction_handler().has_stop_interaction()
    }

    pub fn install_immediate_stop_interaction(&mut self, current_frame: isize) {
        let is_midi = self.is_midi();
        self.interaction_handler_mut()
            .stop_immediately(current_frame, is_midi);
    }

    pub fn schedule_stop_interaction_at(&mut self, frame: isize) {
        self.interaction_handler_mut().schedule_stop_at(frame);
    }

    pub fn reset_interactions(&mut self) {
        self.interaction_handler_mut().reset();
    }

    pub fn prepare_supply(&mut self) {
        let section = self.section();
        // If section start is > 0, the section will take care of applying start fades.
        let enabled_for_start = section.start_frame() == 0;
        // If section end is set, the section will take care of applying end fades.
        let enabled_for_end = section.length().is_none();
        let start_end_handler = self.start_end_handler_mut();
        start_end_handler.set_enabled_for_start(enabled_for_start);
        start_end_handler.set_enabled_for_end(enabled_for_end);
    }

    pub fn reset_for_play(&mut self, looped: bool) {
        self.interaction_handler_mut().reset();
        self.resampler_mut().reset_buffers_and_latency();
        self.time_stretcher_mut().reset_buffers_and_latency();
        self.looper_mut()
            .set_loop_behavior(LoopBehavior::from_bool(looped));
    }

    pub fn get_cycle_at_frame(&self, frame: isize) -> usize {
        self.looper().get_cycle_at_frame(frame)
    }

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) {
        self.looper_mut()
            .keep_playing_until_end_of_current_cycle(pos);
    }

    pub fn set_section_bounds(&mut self, start_frame: usize, length: Option<usize>) {
        self.section_mut().set_bounds(start_frame, length);
    }

    pub fn downbeat_pos_during_recording(&self, timeline: &dyn Timeline) -> DurationInSeconds {
        self.recorder().downbeat_pos_during_recording(timeline)
    }

    pub fn source_frame_rate_in_ready_state(&self) -> Hz {
        self.recorder()
            .frame_rate()
            .expect("recorder couldn't provide frame rate even though clip is in ready state")
    }

    pub fn section_frame_count_in_ready_state(&self) -> usize {
        self.section().frame_count()
    }

    pub fn section_duration_in_ready_state(&self) -> DurationInSeconds {
        self.section().duration()
    }

    fn amplifier(&self) -> &AmplifierTail {
        &self.head
    }

    fn amplifier_mut(&mut self) -> &mut AmplifierTail {
        &mut self.head
    }

    fn interaction_handler(&self) -> &InteractionHandlerTail {
        self.amplifier().supplier()
    }

    fn interaction_handler_mut(&mut self) -> &mut InteractionHandlerTail {
        self.amplifier_mut().supplier_mut()
    }

    fn resampler(&self) -> &ResamplerTail {
        self.interaction_handler().supplier()
    }

    fn resampler_mut(&mut self) -> &mut ResamplerTail {
        self.interaction_handler_mut().supplier_mut()
    }

    fn time_stretcher(&self) -> &TimeStretcherTail {
        self.resampler().supplier()
    }

    fn time_stretcher_mut(&mut self) -> &mut TimeStretcherTail {
        self.resampler_mut().supplier_mut()
    }

    fn downbeat(&self) -> &DownbeatTail {
        self.time_stretcher().supplier()
    }

    fn downbeat_mut(&mut self) -> &mut DownbeatTail {
        self.time_stretcher_mut().supplier_mut()
    }

    fn looper(&self) -> &LooperTail {
        self.downbeat().supplier()
    }

    fn looper_mut(&mut self) -> &mut LooperTail {
        self.downbeat_mut().supplier_mut()
    }

    fn section(&self) -> &SectionTail {
        self.looper().supplier()
    }

    fn section_mut(&mut self) -> &mut SectionTail {
        self.looper_mut().supplier_mut()
    }

    fn start_end_handler(&self) -> &StartEndHandlerTail {
        self.section().supplier()
    }

    fn start_end_handler_mut(&mut self) -> &mut StartEndHandlerTail {
        self.section_mut().supplier_mut()
    }

    fn recorder(&self) -> &RecorderTail {
        self.start_end_handler().supplier()
    }

    // TODO-medium Don't expose.
    pub fn recorder_mut(&mut self) -> &mut RecorderTail {
        self.start_end_handler_mut().supplier_mut()
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

    fn channel_count(&self) -> usize {
        self.head.channel_count()
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

impl PreBufferSourceSkill for SupplierChain {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.head.pre_buffer(request)
    }
}
