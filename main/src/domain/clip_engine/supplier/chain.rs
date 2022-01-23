use crate::domain::clip_engine::supplier::time_stretching::SeriousTimeStretcher;
use crate::domain::clip_engine::supplier::{Looper, Stretcher};
use reaper_medium::OwnedPcmSource;

type Head = StretcherTail;
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
                stretcher
            },
        }
    }

    pub fn head(&self) -> &Head {
        &self.head
    }

    pub fn looper_mut(&mut self) -> &mut LooperTail {
        self.head.supplier_mut()
    }

    pub fn stretcher_mut(&mut self) -> &mut StretcherTail {
        &mut self.head
    }
}
