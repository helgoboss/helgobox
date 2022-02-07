use crate::buffer::AudioBufMut;
use crate::file_util::get_path_for_new_media_file;
use crate::{
    clip_timeline, AudioBuf, AudioSupplier, ClipRecordInput, ExactFrameCount, MidiSupplier,
    OwnedAudioBuffer, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use reaper_high::{OwnedSource, Project, Reaper};
use reaper_low::raw::{
    midi_realtime_write_struct_t, PCM_SINK_EXT_CREATESOURCE, PCM_SOURCE_EXT_ADDMIDIEVENTS,
};
use reaper_low::{raw, PCM_source};
use reaper_medium::{
    BorrowedMidiEventList, Hz, OwnedPcmSink, OwnedPcmSource, PcmSource, PositionInSeconds,
    ReaperString,
};
use std::ffi::CString;
use std::ptr::{null, null_mut, NonNull};
use std::{cmp, mem};

#[derive(Debug)]
pub struct Recorder {
    source: Option<OwnedPcmSource>,
    old_source: Option<OwnedPcmSource>,
    temporary_audio_buffer: OwnedAudioBuffer,
    sink: Option<OwnedPcmSink>,
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
    pub fn new(source: Option<OwnedPcmSource>) -> Self {
        Self {
            source,
            old_source: None,
            temporary_audio_buffer: OwnedAudioBuffer::new(2, 48000 * 2),
            sink: None,
            next_record_start_frame: 0,
        }
    }

    pub fn source(&self) -> Option<&OwnedPcmSource> {
        self.source.as_ref()
    }

    pub fn source_mut(&mut self) -> Option<&mut OwnedPcmSource> {
        self.source.as_mut()
    }

    pub fn prepare_recording(&mut self, input: ClipRecordInput, project: Option<Project>) {
        use ClipRecordInput::*;
        let new_supplier = match input {
            Midi => get_empty_midi_source(),
            Audio => {
                let proj_ptr = project.map(|p| p.raw().as_ptr()).unwrap_or(null_mut());
                let file_name = get_path_for_new_media_file("clip-audio", "wav", project);
                let file_name_str = file_name.to_str().unwrap();
                let file_name_c_string = CString::new(file_name_str).unwrap();
                unsafe {
                    let sink = Reaper::get().medium_reaper().low().PCM_Sink_CreateEx(
                        proj_ptr,
                        file_name_c_string.as_ptr(),
                        null(),
                        0,
                        2,
                        48000,
                        false,
                    );
                    let sink = NonNull::new(sink).expect("PCM_Sink_CreateEx returned null");
                    let sink = OwnedPcmSink::from_raw(sink);
                    let mut pcm_source: *mut raw::PCM_source = null_mut();
                    sink.as_ref().as_ref().Extended(
                        PCM_SINK_EXT_CREATESOURCE,
                        &mut pcm_source as *mut _ as *mut _,
                        null_mut(),
                        null_mut(),
                    );
                    let pcm_source =
                        NonNull::new(pcm_source).expect("PCM sink didn't create a source");
                    let pcm_source = OwnedPcmSource::from_raw(pcm_source);
                    self.sink = Some(sink);
                    pcm_source
                }
            }
        };
        // TODO-high Just replacing is not a good idea. Fade outs?
        self.old_source = self.source.replace(new_supplier);
    }

    pub fn commit_recording(&mut self) -> &OwnedPcmSource {
        self.old_source = None;
        // TODO-high
        self.source().unwrap()
    }

    pub fn rollback_recording(&mut self) {
        self.source = self.old_source.take();
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
        let source = match self.source_mut() {
            None => return,
            Some(s) => s,
        };
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
            source.extended(
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
    // TODO-high We absolutely need the permanent section supplier, then we can play the
    //  source correctly positioned and with correct length even the source is too long
    //  and starts too early.
    // The following seems to be the absolute minimum to create the shortest possible MIDI clip
    // (which still is longer than zero).
    let chunk = "\
        HASDATA 1 960 QN\n\
        E 1 b0 7b 00\n\
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
        let source = match self.source_mut() {
            // Not called when recording.
            None => {
                return SupplyResponse {
                    num_frames_written: 0,
                    num_frames_consumed: 0,
                    next_inner_frame: None,
                }
            }
            Some(s) => s,
        };
        return source.supply_audio(request, dest_buffer);
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
        self.source.as_ref().map(|s| s.channel_count()).unwrap_or(0)
    }
}

impl MidiSupplier for Recorder {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let source = match self.source_mut() {
            None => {
                return SupplyResponse {
                    num_frames_written: 0,
                    num_frames_consumed: 0,
                    next_inner_frame: None,
                }
            }
            Some(s) => s,
        };
        source.supply_midi(request, event_list)
    }
}

impl ExactFrameCount for Recorder {
    fn frame_count(&self) -> usize {
        self.source().map(|s| s.frame_count()).unwrap_or(0)
    }
}

impl WithFrameRate for Recorder {
    fn frame_rate(&self) -> Option<Hz> {
        self.source.as_ref().and_then(|s| s.frame_rate())
    }
}
