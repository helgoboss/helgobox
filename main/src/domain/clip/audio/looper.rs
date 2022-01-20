use crate::domain::clip::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip::buffer::{AudioBufMut, OwnedAudioBuffer};
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
        let start_frame = request.start_frame % supplier_frame_count;
        let request = SupplyAudioRequest {
            start_frame,
            ..*request
        };
        let end_frame = start_frame + dest_buffer.frame_count();
        if end_frame > supplier_frame_count {
            // The requested block covers the border between two cycles.
            let num_frames_at_end = supplier_frame_count - start_frame;
            self.supplier
                .supply_audio(&request, &mut dest_buffer.slice_mut(..num_frames_at_end));
            let start_request = SupplyAudioRequest {
                start_frame: 0,
                ..request
            };
            self.supplier.supply_audio(
                &start_request,
                &mut dest_buffer.slice_mut(num_frames_at_end..),
            );
        } else {
            // The requested block is within one cycle.
            self.supplier.supply_audio(&request, dest_buffer);
        }
        SupplyAudioResponse {
            num_frames_written: dest_buffer.frame_count(),
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
