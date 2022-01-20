use crate::domain::clip::audio::{AudioSupplier, Ctx, SupplyAudioRequest, SupplyAudioResponse};
use crate::domain::clip::buffer::AudioBufMut;
use reaper_medium::Hz;

#[derive(Debug)]
pub struct Resampler;

impl<'a, S: AudioSupplier> AudioSupplier for Ctx<'a, Resampler, S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        let request = SupplyAudioRequest {
            dest_sample_rate: Hz::new(request.dest_sample_rate.get() / self.tempo_factor),
            ..*request
        };
        self.supplier.supply_audio(&request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }

    fn frame_count(&self) -> usize {
        (self.supplier.frame_count() as f64 / self.tempo_factor).round() as usize
    }

    fn sample_rate(&self) -> Hz {
        self.supplier.sample_rate()
    }
}
