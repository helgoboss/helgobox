use crate::domain::clip_engine::supplier::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::supplier::{Looper, Stretcher, Suspender};
use crate::domain::clip_engine::{FlexibleSource, StretchAudioMode};
use reaper_medium::OwnedPcmSource;

type Head = SuspenderTail;
type SuspenderTail = Suspender<StretcherTail>;
type StretcherTail = Stretcher<LooperTail>;
type LooperTail = Looper<FlexibleSourceTail>;
type FlexibleSourceTail = FlexibleSource<ReaperSourceTail>;
type ReaperSourceTail = OwnedPcmSource;

pub struct ClipSupplierChain {
    head: Head,
}

impl ClipSupplierChain {
    pub fn new(reaper_source: OwnedPcmSource) -> Self {
        Self {
            head: {
                let mut flexible_source = FlexibleSource::new(reaper_source);
                let mut looper = Looper::new(flexible_source);
                let mut stretcher = Stretcher::new(looper);
                Suspender::new(stretcher)
            },
        }
    }

    pub fn reset(&mut self) {
        self.suspender_mut().reset();
        self.stretcher_mut().reset();
        self.looper_mut().reset();
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

    pub fn stretcher(&self) -> &StretcherTail {
        self.head.supplier()
    }

    pub fn stretcher_mut(&mut self) -> &mut StretcherTail {
        self.head.supplier_mut()
    }

    pub fn looper(&self) -> &LooperTail {
        self.stretcher().supplier()
    }

    pub fn looper_mut(&mut self) -> &mut LooperTail {
        self.stretcher_mut().supplier_mut()
    }

    pub fn flexible_source(&self) -> &FlexibleSourceTail {
        self.looper().supplier()
    }

    pub fn flexible_source_mut(&mut self) -> &mut FlexibleSourceTail {
        self.looper_mut().supplier_mut()
    }

    pub fn reaper_source(&self) -> &ReaperSourceTail {
        self.flexible_source().supplier()
    }

    pub fn reaper_source_mut(&mut self) -> &mut ReaperSourceTail {
        self.flexible_source_mut().supplier_mut()
    }
}
