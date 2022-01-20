use crate::domain::clip::audio::{AudioSupplier, Ctx, SupplyAudioRequest, SupplyAudioResponse};
use crate::domain::clip::buffer::AudioBufMut;
use reaper_medium::Hz;

#[derive(Debug)]
pub struct SeriousTimeStretcher;

impl<'a, S: AudioSupplier> AudioSupplier for Ctx<'a, SeriousTimeStretcher, S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        todo!()
    }

    fn channel_count(&self) -> usize {
        todo!()
    }

    fn frame_count(&self) -> usize {
        todo!()
    }

    fn sample_rate(&self) -> Hz {
        todo!()
    }
}
