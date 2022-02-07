use crate::supplier::{Fader, Looper};
use crate::{Recorder, Resampler, TimeStretcher, WithFrameRate};
use reaper_medium::{Hz, OwnedPcmSource};

type Head = FaderTail;
type FaderTail = Fader<ResamplerTail>;
type ResamplerTail = Resampler<TimeStretcherTail>;
type TimeStretcherTail = TimeStretcher<LooperTail>;
type LooperTail = Looper<RecorderTail>;
type RecorderTail = Recorder;
type SourceTail = OwnedPcmSource;

#[derive(Debug)]
pub struct SupplierChain {
    head: Head,
}

impl SupplierChain {
    pub fn new(reaper_source: Option<OwnedPcmSource>) -> Self {
        let mut chain = Self {
            head: {
                Fader::new(Resampler::new(TimeStretcher::new(Looper::new(
                    Recorder::new(reaper_source),
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
        looper.set_fades_enabled(true);
        chain
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn head_mut(&mut self) -> &mut Head {
        &mut self.head
    }

    pub fn fader(&self) -> &FaderTail {
        &self.head
    }

    pub fn fader_mut(&mut self) -> &mut FaderTail {
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

    pub fn looper(&self) -> &LooperTail {
        self.time_stretcher().supplier()
    }

    pub fn looper_mut(&mut self) -> &mut LooperTail {
        self.time_stretcher_mut().supplier_mut()
    }

    pub fn recorder(&self) -> &RecorderTail {
        self.looper().supplier()
    }

    pub fn recorder_mut(&mut self) -> &mut RecorderTail {
        self.looper_mut().supplier_mut()
    }

    pub fn source(&self) -> Option<&SourceTail> {
        self.recorder().source()
    }

    pub fn source_in_ready_state(&self) -> &SourceTail {
        self.source()
            .expect("in ready state, REAPER source must be available")
    }

    pub fn source_frame_rate_in_ready_state(&self) -> Hz {
        self.source_in_ready_state()
            .frame_rate()
            .expect("source didn't report frame rate in ready state")
    }
}
