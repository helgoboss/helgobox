use crate::domain::clip::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames, AudioSupplier,
    SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip::buffer::AudioBufMut;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer};

impl AudioSupplier for &BorrowedPcmSource {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        let mut transfer = PcmSourceTransfer::default();
        let time_s = convert_duration_in_frames_to_seconds(request.start_frame, self.sample_rate());
        unsafe {
            transfer.set_nch(dest_buffer.channel_count() as _);
            transfer.set_length(dest_buffer.frame_count() as _);
            transfer.set_sample_rate(request.dest_sample_rate);
            transfer.set_samples(dest_buffer.data_as_mut_ptr());
            transfer.set_time_s(time_s.into());
            self.get_samples(&transfer);
        }
        SupplyAudioResponse {
            num_frames_written: transfer.samples_out() as _,
        }
    }

    fn channel_count(&self) -> usize {
        self.get_num_channels()
            .expect("source doesn't report channel count") as usize
    }

    fn frame_count(&self) -> usize {
        let duration = self.get_length().unwrap_or(DurationInSeconds::ZERO);
        convert_duration_in_seconds_to_frames(duration, self.sample_rate())
    }

    fn sample_rate(&self) -> Hz {
        self.get_sample_rate()
            .expect("source doesn't report a sample rate")
    }
}
