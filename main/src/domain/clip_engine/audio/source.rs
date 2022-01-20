use crate::domain::clip_engine::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    ExactSizeAudioSupplier, SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip_engine::buffer::AudioBufMut;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, OwnedPcmSource, PcmSourceTransfer};

impl AudioSupplier for OwnedPcmSource {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        let mut transfer = PcmSourceTransfer::default();
        let source_sample_rate = self.sample_rate();
        let time_s = convert_duration_in_frames_to_seconds(request.start_frame, source_sample_rate);
        unsafe {
            transfer.set_nch(dest_buffer.channel_count() as _);
            transfer.set_length(dest_buffer.frame_count() as _);
            transfer.set_sample_rate(request.dest_sample_rate);
            transfer.set_samples(dest_buffer.data_as_mut_ptr());
            transfer.set_time_s(time_s.into());
            self.get_samples(&transfer);
        }
        let num_frames_written = transfer.samples_out() as _;
        // The lower the destination sample rate in relation to the source sample rate, the
        // higher the tempo.
        let tempo_factor = source_sample_rate.get() / request.dest_sample_rate.get();
        // The higher the tempo, the more inner source material we effectively grabbed.
        let consumed_frames = (num_frames_written as f64 * tempo_factor).round() as usize;
        let next_frame = request.start_frame + consumed_frames;
        SupplyAudioResponse {
            num_frames_written,
            next_inner_frame: if next_frame < self.frame_count() {
                Some(next_frame)
            } else {
                None
            },
        }
    }

    fn channel_count(&self) -> usize {
        self.get_num_channels()
            .expect("source doesn't report channel count") as usize
    }

    fn sample_rate(&self) -> Hz {
        self.get_sample_rate()
            .expect("source doesn't report a sample rate")
    }
}

impl ExactSizeAudioSupplier for OwnedPcmSource {
    fn frame_count(&self) -> usize {
        let duration = self.get_length().unwrap_or(DurationInSeconds::ZERO);
        convert_duration_in_seconds_to_frames(duration, self.sample_rate())
    }
}
