use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::supplier::{
    AudioSupplier, Ctx, SupplyAudioRequest, SupplyResponse, WithFrameRate,
};
use crossbeam_channel::Receiver;
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::Hz;

#[derive(Debug)]
pub struct SeriousTimeStretcher {
    api: &'static IReaperPitchShift,
}

impl SeriousTimeStretcher {
    pub fn new() -> Self {
        let api = Reaper::get()
            .medium_reaper()
            .low()
            .ReaperGetPitchShiftAPI(REAPER_PITCHSHIFT_API_VER);
        let api = unsafe { &*api };
        Self { api }
    }

    pub fn reset(&mut self) {
        self.api.Reset();
    }
}

impl<'a, S: AudioSupplier + WithFrameRate> AudioSupplier for Ctx<'a, SeriousTimeStretcher, S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let mut total_num_frames_read = 0usize;
        let mut total_num_frames_written = 0usize;
        let source_frame_rate = self.supplier.frame_rate();
        // I think it makes sense to set both the output and the input sample rate to the sample
        // rate of the source. Then the result can be even cached and sample rate & play-rate
        // changes don't need to invalidate the cache.
        // TODO-high However, we need add a resampler on top.
        // TODO-medium Setting this right at the beginning should be enough.
        self.mode.api.set_srate(source_frame_rate.get());
        loop {
            // Fill buffer with a minimum amount of source data (so that we never consume more than
            // necessary).
            let dest_nch = dest_buffer.channel_count();
            self.mode.api.set_nch(dest_nch as _);
            self.mode.api.set_tempo(self.tempo_factor);
            let buffer_frame_count = 128usize;
            let stretch_buffer = self.mode.api.GetBuffer(buffer_frame_count as _);
            let mut stretch_buffer =
                unsafe { AudioBufMut::from_raw(stretch_buffer, dest_nch, buffer_frame_count) };
            let request = SupplyAudioRequest {
                start_frame: request.start_frame + total_num_frames_read as isize,
                dest_sample_rate: source_frame_rate,
            };
            let response = self.supplier.supply_audio(&request, &mut stretch_buffer);
            total_num_frames_read += response.num_frames_written;
            self.mode.api.BufferDone(response.num_frames_written as _);
            // Get samples
            let mut offset_buffer = dest_buffer.slice_mut(total_num_frames_written..);
            let num_frames_written = unsafe {
                self.mode.api.GetSamples(
                    offset_buffer.frame_count() as _,
                    offset_buffer.data_as_mut_ptr(),
                )
            };
            total_num_frames_written += num_frames_written as usize;
            println!(
                "num_frames_read: {}, total_num_frames_read: {}, num_frames_written: {}, total_num_frames_written: {}",
                response.num_frames_written, total_num_frames_read, num_frames_written, total_num_frames_written
            );
            if total_num_frames_written >= dest_buffer.frame_count() {
                // We have enough stretched material.
                break;
            }
        }
        assert_eq!(
            total_num_frames_written,
            dest_buffer.frame_count(),
            "wrote more frames than requested"
        );
        let next_frame = request.start_frame + total_num_frames_read as isize;
        SupplyResponse {
            num_frames_written: total_num_frames_written,
            num_frames_consumed: total_num_frames_read,
            next_inner_frame: Some(next_frame),
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<'a, S: WithFrameRate> WithFrameRate for Ctx<'a, SeriousTimeStretcher, S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}

pub enum StretchWorkerRequest {
    Stretch,
}

/// A function that keeps processing stretch worker requests until the channel of the given receiver
/// is dropped.
pub fn keep_stretching(requests: Receiver<StretchWorkerRequest>) {}
