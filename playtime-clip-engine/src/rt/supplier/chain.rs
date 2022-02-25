use crate::conversion_util::convert_duration_in_seconds_to_frames;
use crate::main::ClipContent;
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::{
    AdHocFader, Downbeat, ExactDuration, ExactFrameCount, LoopBehavior, Looper, Recorder,
    Resampler, Section, StartEndFader, TimeStretcher, WithFrameRate,
};
use crate::rt::ClipInfo;
use crate::ClipEngineResult;
use playtime_api::{
    AudioCacheBehavior, AudioTimeStretchMode, MidiResetMessageRange, PositiveBeat, PositiveSecond,
    TimeStretchMode, VirtualResampleMode,
};
use reaper_high::Project;
use reaper_medium::{Bpm, DurationInSeconds, Hz, PositionInSeconds};

type Head = AdHocFaderTail;
type AdHocFaderTail = AdHocFader<ResamplerTail>;
type ResamplerTail = Resampler<TimeStretcherTail>;
type TimeStretcherTail = TimeStretcher<DownbeatTail>;
type DownbeatTail = Downbeat<LooperTail>;
type LooperTail = Looper<SectionTail>;
type SectionTail = Section<StartEndFaderTail>;
type StartEndFaderTail = StartEndFader<RecorderTail>;
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
                AdHocFader::new(Resampler::new(TimeStretcher::new(Downbeat::new(
                    Looper::new(Section::new(StartEndFader::new(recorder))),
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

    pub fn clear_downbeat(&mut self) {
        self.downbeat_mut().set_downbeat_frame(0);
    }

    pub fn set_audio_fades_enabled_for_source(&mut self, enabled: bool) {
        self.start_end_fader_mut().set_audio_fades_enabled(enabled);
    }

    pub fn set_midi_reset_msg_range_for_section(&mut self, range: MidiResetMessageRange) {
        self.section_mut().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_interaction(&mut self, range: MidiResetMessageRange) {
        self.ad_hoc_fader_mut().set_midi_reset_msg_range(range);
    }

    pub fn set_midi_reset_msg_range_for_source(&mut self, range: MidiResetMessageRange) {
        self.start_end_fader_mut().set_midi_reset_msg_range(range);
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

    pub fn prepare_supply(&mut self) {
        let section = self.section();
        // If section start is > 0, the section will take care of applying start fades.
        let enabled_for_start = section.start_frame() == 0;
        // If section end is set, the section will take care of applying end fades.
        let enabled_for_end = section.length().is_none();
        let start_end_fader = self.start_end_fader_mut();
        start_end_fader.set_enabled_for_start(enabled_for_start);
        start_end_fader.set_enabled_for_end(enabled_for_end);
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn head_mut(&mut self) -> &mut Head {
        &mut self.head
    }

    pub fn ad_hoc_fader(&self) -> &AdHocFaderTail {
        &self.head
    }

    pub fn ad_hoc_fader_mut(&mut self) -> &mut AdHocFaderTail {
        &mut self.head
    }

    pub fn resampler(&self) -> &ResamplerTail {
        self.head.supplier()
    }

    pub fn resampler_mut(&mut self) -> &mut ResamplerTail {
        self.head.supplier_mut()
    }

    pub fn time_stretcher(&self) -> &TimeStretcherTail {
        self.resampler().supplier()
    }

    pub fn time_stretcher_mut(&mut self) -> &mut TimeStretcherTail {
        self.resampler_mut().supplier_mut()
    }

    pub fn downbeat(&self) -> &DownbeatTail {
        self.time_stretcher().supplier()
    }

    pub fn downbeat_mut(&mut self) -> &mut DownbeatTail {
        self.time_stretcher_mut().supplier_mut()
    }

    pub fn looper(&self) -> &LooperTail {
        self.downbeat().supplier()
    }

    pub fn looper_mut(&mut self) -> &mut LooperTail {
        self.downbeat_mut().supplier_mut()
    }

    pub fn section(&self) -> &SectionTail {
        self.looper().supplier()
    }

    pub fn section_mut(&mut self) -> &mut SectionTail {
        self.looper_mut().supplier_mut()
    }

    pub fn start_end_fader(&self) -> &StartEndFaderTail {
        self.section().supplier()
    }

    pub fn start_end_fader_mut(&mut self) -> &mut StartEndFaderTail {
        self.section_mut().supplier_mut()
    }

    pub fn recorder(&self) -> &RecorderTail {
        self.start_end_fader().supplier()
    }

    pub fn recorder_mut(&mut self) -> &mut RecorderTail {
        self.start_end_fader_mut().supplier_mut()
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

    pub fn clip_info(&self) -> Option<ClipInfo> {
        self.recorder().clip_info()
    }

    pub fn clip_content(&self, project: Option<Project>) -> Option<ClipContent> {
        self.recorder().clip_content(project)
    }
}
