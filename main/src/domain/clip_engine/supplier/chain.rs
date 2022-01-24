use crate::domain::clip_engine::supplier::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::supplier::{Looper, Stretcher, Suspender};
use crate::domain::clip_engine::StretchAudioMode;
use reaper_medium::OwnedPcmSource;

type Head = SuspenderTail;
type SuspenderTail = Suspender<StretcherTail>;
type StretcherTail = Stretcher<LooperTail>;
type LooperTail = Looper<SourceTail>;
type SourceTail = OwnedPcmSource;

pub struct ClipSupplierChain {
    head: Head,
}

impl ClipSupplierChain {
    pub fn new(source: OwnedPcmSource) -> Self {
        Self {
            head: {
                let mut looper = Looper::new(source);
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

    pub fn source(&self) -> &SourceTail {
        self.looper().supplier()
    }

    pub fn source_mut(&mut self) -> &mut SourceTail {
        self.looper_mut().supplier_mut()
    }
}
