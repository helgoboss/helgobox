use crate::domain::clip_engine::audio::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    convert_position_in_frames_to_seconds, supply_source_material, AudioSupplier,
    ExactSizeAudioSupplier, SourceMaterialRequest, SupplyAudioRequest, SupplyAudioResponse,
};
use crate::domain::clip_engine::buffer::AudioBufMut;
use reaper_medium::{BorrowedPcmSource, DurationInSeconds, Hz, OwnedPcmSource, PcmSourceTransfer};

impl AudioSupplier for OwnedPcmSource {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyAudioResponse {
        supply_source_material(
            request,
            dest_buffer,
            self.sample_rate(),
            self.frame_count(),
            |input| transfer_samples(self, input),
        )
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

fn transfer_samples(source: &OwnedPcmSource, mut req: SourceMaterialRequest) -> usize {
    let mut transfer = PcmSourceTransfer::default();
    let time_s = convert_duration_in_frames_to_seconds(req.start_frame, req.source_sample_rate);
    unsafe {
        transfer.set_nch(req.dest_buffer.channel_count() as _);
        transfer.set_length(req.dest_buffer.frame_count() as _);
        transfer.set_sample_rate(req.dest_sample_rate);
        transfer.set_samples(req.dest_buffer.data_as_mut_ptr());
        transfer.set_time_s(time_s.into());
        source.get_samples(&transfer);
        transfer.samples_out() as usize
    }
}
