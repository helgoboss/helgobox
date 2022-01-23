use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::supplier::{
    AudioSupplier, Ctx, ExactFrameCount, SupplyAudioRequest, SupplyResponse, WithFrameRate,
};
use reaper_medium::Hz;

#[derive(Debug)]
pub struct Resampler;

impl Resampler {
    pub fn reset(&mut self) {
        // TODO-high As soon as we do our own resampling, we want to reset the resampler here.
    }
}

impl<'a, S: AudioSupplier> AudioSupplier for Ctx<'a, Resampler, S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        // TODO-high At lower sample rates there are sometimes clicks. Rounding errors?
        let request = SupplyAudioRequest {
            dest_sample_rate: Hz::new(request.dest_sample_rate.get() / self.tempo_factor),
            ..*request
        };
        self.supplier.supply_audio(&request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<'a, S: WithFrameRate> WithFrameRate for Ctx<'a, Resampler, S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}
