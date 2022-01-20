use crate::domain::clip::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip::buffer::{AudioBufMut, OwnedAudioBuffer};
use core::cmp;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

pub struct AudioStretcher<S: AudioSupplier> {
    enabled: bool,
    supplier: S,
    tempo_factor: f64,
}

impl<S: AudioSupplier> AudioStretcher<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            enabled: false,
            supplier,
            tempo_factor: 1.0,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.tempo_factor = tempo_factor;
    }
}

impl<S: AudioSupplier> AudioSupplier for AudioStretcher<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        if !self.enabled {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
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
