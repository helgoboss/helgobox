use crate::supplier::{Looper, Suspender};
use crate::{Recorder, Resampler, TimeStretcher};
use reaper_medium::OwnedPcmSource;

type Head = SuspenderTail;
type SuspenderTail = Suspender<ResamplerTail>;
type ResamplerTail = Resampler<TimeStretcherTail>;
type TimeStretcherTail = TimeStretcher<LooperTail>;
type LooperTail = Looper<RecorderTail>;
type RecorderTail = Recorder<ReaperSourceTail>;
type ReaperSourceTail = OwnedPcmSource;

#[derive(Debug)]
pub struct ClipSupplierChain {
    head: Head,
}

impl ClipSupplierChain {
    pub fn new(reaper_source: OwnedPcmSource) -> Self {
        Self {
            head: {
                Suspender::new(Resampler::new(TimeStretcher::new(Looper::new(
                    Recorder::new(reaper_source),
                ))))
            },
        }
    }

    pub fn reset_for_play(&mut self) {
        self.suspender_mut().reset();
        self.resampler_mut().reset_buffers_and_latency();
        self.time_stretcher_mut().reset_buffers_and_latency();
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn head_mut(&mut self) -> &mut Head {
        &mut self.head
    }

    pub fn suspender(&self) -> &SuspenderTail {
        &self.head
    }

    pub fn suspender_mut(&mut self) -> &mut SuspenderTail {
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

    pub fn reaper_source(&self) -> &ReaperSourceTail {
        self.recorder().supplier()
    }

    pub fn reaper_source_mut(&mut self) -> &mut ReaperSourceTail {
        self.recorder_mut().supplier_mut()
    }
}
