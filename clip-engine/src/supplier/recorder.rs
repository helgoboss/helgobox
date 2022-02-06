use crate::buffer::AudioBufMut;
use crate::{
    clip_timeline, AudioBuf, AudioSupplier, ExactFrameCount, MidiSupplier, OwnedAudioBuffer,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use reaper_high::OwnedSource;
use reaper_low::raw::{midi_realtime_write_struct_t, PCM_SOURCE_EXT_ADDMIDIEVENTS};
use reaper_medium::{BorrowedMidiEventList, Hz, OwnedPcmSource, PositionInSeconds};
use std::cmp;
use std::ptr::null_mut;

#[derive(Debug)]
pub struct Recorder {
    supplier: OwnedPcmSource,
    temporary_audio_buffer: OwnedAudioBuffer,
    next_record_start_frame: usize,
}

#[derive(Copy, Clone)]
pub struct WriteMidiRequest<'a> {
    pub input_sample_rate: Hz,
    pub block_length: usize,
    pub events: &'a BorrowedMidiEventList,
}

#[derive(Copy, Clone)]
pub struct WriteAudioRequest<'a> {
    pub input_sample_rate: Hz,
    pub block_length: usize,
    pub left_buffer: AudioBuf<'a>,
    pub right_buffer: AudioBuf<'a>,
}

impl Recorder {
    pub fn new(source: OwnedPcmSource) -> Self {
        Self {
            supplier: source,
            temporary_audio_buffer: OwnedAudioBuffer::new(2, 48000 * 2),
            next_record_start_frame: 0,
        }
    }

    pub fn supplier(&self) -> &OwnedPcmSource {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut OwnedPcmSource {
        &mut self.supplier
    }

    pub fn prepare_recording(&mut self) {
        // TODO-high Just replacing is not a good idea. Fade outs?
        self.supplier = get_empty_midi_source();
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

    pub fn write_midi(&mut self, request: WriteMidiRequest, pos: PositionInSeconds) {
        let mut write_struct = midi_realtime_write_struct_t {
            global_time: pos.get(),
            srate: request.input_sample_rate.get(),
            item_playrate: 1.0,
            global_item_time: 0.0,
            length: request.block_length as _,
            // Overdub
            overwritemode: 0,
            events: unsafe { request.events.as_ptr().as_mut() },
            latency: 0.0,
            // Not used
            overwrite_actives: null_mut(),
        };
        unsafe {
            self.supplier.extended(
                PCM_SOURCE_EXT_ADDMIDIEVENTS as _,
                &mut write_struct as *mut _ as _,
                null_mut(),
                null_mut(),
            );
        }
    }
}

/// Returns an empty MIDI source prepared for recording.
pub fn get_empty_midi_source() -> OwnedPcmSource {
    // TODO-high Also implement for audio recording.
    let mut source = OwnedSource::from_type("MIDI").unwrap();
    // TODO-high Only keep necessary parts of the chunk
    // TODO-high We absolutely need the permanent section supplier, then we can play the
    //  source correctly positioned and with correct length even the source is too long
    //  and starts too early.
    let chunk = "\
                HASDATA 1 960 QN\n\
                CCINTERP 32\n\
                POOLEDEVTS {1F408000-28E4-46FA-9CB8-935A213C5904}\n\
                E 1 b0 7b 00\n\
                CCINTERP 32\n\
                CHASE_CC_TAKEOFFS 1\n\
                GUID {1A129921-1EC6-4C57-B340-95F076A6B9FF}\n\
                IGNTEMPO 0 120 4 4\n\
                SRCCOLOR 647\n\
                VELLANE 141 274 0\n\
            >\n\
            ";
    source
        .set_state_chunk("<SOURCE MIDI\n", String::from(chunk))
        .unwrap();
    source.into_raw()
}

impl AudioSupplier for Recorder {
    fn supply_audio(
        &mut self,
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

impl MidiSupplier for Recorder {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        self.supplier.supply_midi(request, event_list)
    }
}

impl ExactFrameCount for Recorder {
    fn frame_count(&self) -> usize {
        self.supplier.frame_count()
    }
}

impl WithFrameRate for Recorder {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}
