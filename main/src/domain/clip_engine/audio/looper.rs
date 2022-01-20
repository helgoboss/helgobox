use crate::domain::clip_engine::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip_engine::buffer::{AudioBufMut, OwnedAudioBuffer};
use core::cmp;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

pub struct AudioLooper<S: AudioSupplier> {
    enabled: bool,
    supplier: S,
}

impl<S: AudioSupplier> AudioLooper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            enabled: false,
            supplier,
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

impl<S: AudioSupplier> AudioSupplier for AudioLooper<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        if !self.enabled {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let start_frame = request.start_frame % supplier_frame_count;
        let request = SupplyAudioRequest {
            start_frame,
            ..*request
        };
        let response = self.supplier.supply_audio(&request, dest_buffer);
        if response.num_frames_written == dest_buffer.frame_count() {
            // Didn't cross the end yet. Nothing else to do.
            return response;
        }
        // Crossed the end. We need to fill the rest with material from the beginning of the source.
        let second_request = SupplyAudioRequest {
            start_frame: 0,
            ..request
        };
        let second_response = self.supplier.supply_audio(
            &second_request,
            &mut dest_buffer.slice_mut(response.num_frames_written..),
        );
        SupplyAudioResponse {
            num_frames_written: dest_buffer.frame_count(),
            next_inner_frame: second_response.next_inner_frame,
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }

    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }

    fn sample_rate(&self) -> Hz {
        self.supplier.sample_rate()
    }
}
