use crate::{Fx, FxDesc, Reaper, Track, TrackDesc};
use reaper_medium::{AddFxBehavior, ReaperFunctionError, ReaperStringArg, TrackFxChainType};
use std::iter::FusedIterator;

// TODO-high Monitoring context
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FxChainDesc {
    track_desc: TrackDesc,
    kind: TrackFxChainType,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FxChain<'a> {
    track: Track<'a>,
    kind: TrackFxChainType,
}

impl FxChainDesc {
    pub fn new(track_desc: TrackDesc, kind: TrackFxChainType) -> Self {
        Self { track_desc, kind }
    }

    pub fn resolve(&self) -> Option<FxChain> {
        let fx_chain = FxChain {
            track: self.track_desc.resolve()?,
            kind: self.kind,
        };
        Some(fx_chain)
    }
}

impl<'a> FxChain<'a> {
    pub fn add_fx_by_name<'b>(
        &mut self,
        name: impl Into<ReaperStringArg<'b>>,
        behavior: AddFxBehavior,
    ) -> Result<FxDesc, ReaperFunctionError> {
        let r = Reaper::get().medium_reaper();
        let index =
            unsafe { r.track_fx_add_by_name_add(self.track.raw(), name, self.kind, behavior)? };
        let fx = Fx::new(*self, index);
        Ok(FxDesc::new(self.desc(), fx.guid()))
    }

    pub fn fxs(
        &self,
    ) -> impl Iterator<Item = Fx> + ExactSizeIterator + DoubleEndedIterator + FusedIterator {
        (0..self.fx_count()).map(|i| Fx::new(*self, i))
    }

    pub fn fx_count(&self) -> u32 {
        let r = Reaper::get().medium_reaper();
        match self.kind {
            TrackFxChainType::NormalFxChain => unsafe { r.track_fx_get_count(self.track.raw()) },
            TrackFxChainType::InputFxChain => unsafe { r.track_fx_get_rec_count(self.track.raw()) },
        }
    }

    pub fn kind(&self) -> TrackFxChainType {
        self.kind
    }

    pub fn desc(&self) -> FxChainDesc {
        FxChainDesc::new(self.track.desc(), self.kind)
    }

    pub fn track(&self) -> Track {
        self.track
    }
}
