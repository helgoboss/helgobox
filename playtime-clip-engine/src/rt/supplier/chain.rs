use crate::main::ClipContent;
use crate::rt::supplier::{
    AdHocFader, Downbeat, ExactDuration, ExactFrameCount, Looper, Recorder, Resampler, Section,
    StartEndFader, TimeStretcher, WithFrameRate,
};
use crate::rt::ClipInfo;
use reaper_high::Project;
use reaper_medium::{DurationInSeconds, Hz};

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
        // Configure looper
        let looper = chain.looper_mut();
        looper.set_enabled(true);
        // Configure downbeat
        let downbeat = chain.downbeat_mut();
        downbeat.set_enabled(true);
        // Configure recorder
        let recorder = chain.recorder_mut();
        // recorder.enable_cache();
        recorder.set_pre_buffering_enabled(true);
        chain
    }

    pub fn prepare_supply(&mut self, auto_fades_enabled: bool) {
        let (fade_in_enabled, fade_out_enabled) = if auto_fades_enabled {
            let section = self.section();
            (section.start_frame() == 0, section.length().is_none())
        } else {
            (false, false)
        };
        let start_end_fader = self.start_end_fader_mut();
        start_end_fader.set_fade_in_enabled(fade_in_enabled);
        start_end_fader.set_fade_out_enabled(fade_out_enabled);
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
