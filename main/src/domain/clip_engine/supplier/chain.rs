use crate::domain::clip_engine::supplier::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::supplier::{Looper, Stretcher, Suspender};
use reaper_medium::OwnedPcmSource;

type Head = SuspenderTail;
type SuspenderTail = Suspender<StretcherTail>;
type StretcherTail = Stretcher<LooperTail>;
type LooperTail = Looper<OwnedPcmSource>;

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

    pub fn suspender_mut(&mut self) -> &mut SuspenderTail {
        &mut self.head
    }

    pub fn stretcher_mut(&mut self) -> &mut StretcherTail {
        self.head.supplier_mut()
    }
    pub fn looper_mut(&mut self) -> &mut LooperTail {
        self.stretcher_mut().supplier_mut()
    }
}
