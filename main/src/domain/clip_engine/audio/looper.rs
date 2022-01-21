use crate::domain::clip_engine::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    ExactSizeAudioSupplier, SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip_engine::buffer::{AudioBufMut, OwnedAudioBuffer};
use core::cmp;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

pub struct AudioLooper<S: ExactSizeAudioSupplier> {
    enabled: bool,
    fades_enabled: bool,
    supplier: S,
}

impl<S: ExactSizeAudioSupplier> AudioLooper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            enabled: false,
            fades_enabled: false,
            supplier,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_fades_enabled(&mut self, fades_enabled: bool) {
        self.fades_enabled = fades_enabled;
    }
}

impl<S: ExactSizeAudioSupplier> AudioSupplier for AudioLooper<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        if !self.enabled || request.start_frame < 0 {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
        let start_frame = request.start_frame as usize;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyAudioRequest {
            start_frame: modulo_start_frame as isize,
            ..*request
        };
        let modulo_response = self.supplier.supply_audio(&modulo_request, dest_buffer);
        let final_response = if modulo_response.num_frames_written == dest_buffer.frame_count() {
            // Didn't cross the end yet. Nothing else to do.
            SupplyAudioResponse {
                num_frames_written: modulo_response.num_frames_written,
                next_inner_frame: modulo_response.next_inner_frame.map(|f| {
                    let num_consumed_frames = f - modulo_start_frame as isize;
                    request.start_frame + num_consumed_frames
                }),
            }
        } else {
            // Crossed the end. We need to fill the rest with material from the beginning of the source.
            let start_request = SupplyAudioRequest {
                start_frame: 0,
                ..modulo_request
            };
            let start_response = self.supplier.supply_audio(
                &start_request,
                &mut dest_buffer.slice_mut(modulo_response.num_frames_written..),
            );
            SupplyAudioResponse {
                num_frames_written: dest_buffer.frame_count(),
                next_inner_frame: start_response.next_inner_frame.map(|f| {
                    let num_consumed_frames = f - modulo_start_frame as isize;
                    request.start_frame + num_consumed_frames
                }),
            }
        };
        if self.fades_enabled {
            dest_buffer.modify_frames(|frame, sample| {
                let factor = calc_volume_factor_at(start_frame + frame, supplier_frame_count);
                sample * factor
            });
        }
        final_response
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }

    fn sample_rate(&self) -> Hz {
        self.supplier.sample_rate()
    }
}

fn calc_volume_factor_at(frame: usize, frame_count: usize) -> f64 {
    let modulo_frame = frame % frame_count;
    let distance_to_end = frame_count - modulo_frame;
    if distance_to_end < FADE_LENGTH {
        // Approaching loop end: Fade out
        return distance_to_end as f64 / FADE_LENGTH as f64;
    }
    if frame >= frame_count && modulo_frame < FADE_LENGTH {
        // Continuing at loop start: Fade in
        return modulo_frame as f64 / FADE_LENGTH as f64;
    }
    return 1.0;
}

// 0.01s = 10ms at 48 kHz
const FADE_LENGTH: usize = 480;
