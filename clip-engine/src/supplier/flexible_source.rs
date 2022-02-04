use crate::buffer::AudioBufMut;
use crate::{
    AudioBuf, AudioSupplier, ExactFrameCount, MidiSupplier, OwnedAudioBuffer, SupplyAudioRequest,
    SupplyMidiRequest, SupplyResponse, WithFrameRate, WriteAudioRequest,
};
use reaper_medium::{BorrowedMidiEventList, Hz};
use std::cmp;

#[derive(Debug)]
pub struct FlexibleSource<S> {
    supplier: S,
    temporary_audio_buffer: OwnedAudioBuffer,
    next_record_start_frame: usize,
}

impl<S> FlexibleSource<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            temporary_audio_buffer: OwnedAudioBuffer::new(2, 48000 * 2),
            next_record_start_frame: 0,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn write_audio(&mut self, request: WriteAudioRequest) {
        // // TODO-high Obviously just some experiments.
        let start_frame = self.next_record_start_frame;
        let mut out_buf = self.temporary_audio_buffer.to_buf_mut();
        let out_channel_count = out_buf.channel_count();
        let ideal_end_frame = start_frame + request.block_length;
        let end_frame = cmp::min(ideal_end_frame, out_buf.frame_count());
        let num_frames_written = end_frame - start_frame;
        let mut out_buf_slice = out_buf.data_as_mut_slice();
        let left_buf_slice = request.left_buffer.data_as_slice();
        let right_buf_slice = request.right_buffer.data_as_slice();
        for i in 0..num_frames_written {
            out_buf_slice[start_frame * out_channel_count + i * out_channel_count + 0] =
                left_buf_slice[i];
            // out_buf_slice[start_frame + i * out_channel_count + 0] = left_buf_slice[i];
            // out_buf_slice[start_frame + i * out_channel_count + 1] = right_buf_slice[i];
        }
        // request
        //     .left_buffer
        //     .slice(..num_frames_written)
        //     .copy_to(&mut out_buf.slice_mut(start_frame..end_frame));
        self.next_record_start_frame += num_frames_written;
    }
}

impl<S: AudioSupplier> AudioSupplier for FlexibleSource<S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        return self.supplier.supply_audio(request, dest_buffer);
        // // TODO-high Obviously just some experiments.
        // let temp_buf = self.temporary_audio_buffer.to_buf();
        // if request.start_frame < 0 {
        //     return self.supplier.supply_audio(request, dest_buffer);
        // }
        // let mod_start_frame = request.start_frame as usize % temp_buf.frame_count();
        // let ideal_end_frame = mod_start_frame + dest_buffer.frame_count();
        // let end_frame = cmp::min(ideal_end_frame, temp_buf.frame_count());
        // let num_frames_to_write = end_frame - mod_start_frame;
        // temp_buf
        //     .slice(mod_start_frame..end_frame)
        //     .copy_to(&mut dest_buffer.slice_mut(..num_frames_to_write))
        //     .unwrap();
        // let num_frames_written = dest_buffer.frame_count();
        // SupplyResponse {
        //     num_frames_written,
        //     num_frames_consumed: num_frames_written,
        //     next_inner_frame: Some(request.start_frame + num_frames_written as isize),
        // }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier> MidiSupplier for FlexibleSource<S> {
    fn supply_midi(
        &self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        self.supplier.supply_midi(request, event_list)
    }
}

impl<S: ExactFrameCount> ExactFrameCount for FlexibleSource<S> {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for FlexibleSource<S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}
