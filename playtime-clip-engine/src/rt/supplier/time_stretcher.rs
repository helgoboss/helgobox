use crate::rt::buffer::AudioBufMut;
use crate::rt::supplier::{
    AudioSupplier, SupplyAudioRequest, SupplyResponse, SupplyResponseStatus, WithFrameRate,
};
use crate::rt::supplier::{
    MidiSupplier, PreBufferFillRequest, PreBufferSourceSkill, SupplyMidiRequest, SupplyRequestInfo,
};
use crossbeam_channel::Receiver;
use playtime_api::VirtualTimeStretchMode;
use reaper_high::Reaper;
use reaper_low::raw::REAPER_PITCHSHIFT_API_VER;
use reaper_medium::{BorrowedMidiEventList, Hz, OwnedReaperPitchShift};

#[derive(Debug)]
pub struct TimeStretcher<S> {
    api: OwnedReaperPitchShift,
    supplier: S,
    enabled: bool,
    responsible_for_audio_time_stretching: bool,
    tempo_factor: f64,
}

impl<S> TimeStretcher<S> {
    pub fn new(supplier: S) -> Self {
        let api = Reaper::get()
            .medium_reaper()
            .reaper_get_pitch_shift_api(REAPER_PITCHSHIFT_API_VER)
            .expect("couldn't get pitch shift API in correct version");
        Self {
            api,
            supplier,
            enabled: false,
            responsible_for_audio_time_stretching: false,
            tempo_factor: 1.0,
        }
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    /// Decides whether the time stretcher should take the tempo factor into account for audio.
    /// Usually it does.
    pub fn set_responsible_for_audio_time_stretching(&mut self, responsible: bool) {
        self.responsible_for_audio_time_stretching = responsible;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_mode(&mut self, mode: VirtualTimeStretchMode) {
        use VirtualTimeStretchMode::*;
        let raw_quality_param = match mode {
            ProjectDefault => -1i32,
            ReaperMode(m) => (m.mode << 16 + m.sub_mode) as i32,
        };
        self.api
            .as_mut()
            .as_mut()
            .SetQualityParameter(raw_quality_param);
    }

    pub fn set_tempo_factor(&mut self, tempo_factor: f64) {
        self.tempo_factor = tempo_factor;
    }

    pub fn reset_buffers_and_latency(&mut self) {
        self.api.as_mut().as_mut().Reset();
    }
}

impl<S: AudioSupplier + WithFrameRate> AudioSupplier for TimeStretcher<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        if !self.enabled || !self.responsible_for_audio_time_stretching {
            return self.supplier.supply_audio(&request, dest_buffer);
        }
        let source_frame_rate = match self.supplier.frame_rate() {
            None => {
                // Nothing to stretch at the moment.
                return self.supplier.supply_audio(&request, dest_buffer);
            }
            Some(r) => r,
        };
        debug_assert_eq!(request.dest_sample_rate, source_frame_rate);
        let mut total_num_frames_consumed = 0usize;
        let mut total_num_frames_written = 0usize;
        // I think it makes sense to set both the output and the input sample rate to the sample
        // rate of the source. Then the result could be even cached and sample rate & play-rate
        // changes don't need to invalidate the cache.
        // TODO-medium Setting this right at the beginning should be enough.
        let api = self.api.as_mut().as_mut();
        api.set_srate(source_frame_rate.get());
        let dest_nch = dest_buffer.channel_count();
        api.set_nch(dest_nch as _);
        api.set_tempo(self.tempo_factor);
        let reached_end = loop {
            // Get time stretcher buffer.
            let buffer_frame_count = 128usize;
            let stretch_buffer = api.GetBuffer(buffer_frame_count as _);
            let mut stretch_buffer =
                unsafe { AudioBufMut::from_raw(stretch_buffer, dest_nch, buffer_frame_count) };
            // Fill buffer with a minimum amount of source data (so that we never consume more than
            // necessary).
            let inner_request = SupplyAudioRequest {
                start_frame: request.start_frame + total_num_frames_consumed as isize,
                dest_sample_rate: source_frame_rate,
                info: SupplyRequestInfo {
                    // Here we should not add total_num_frames_written because it doesn't grow
                    // proportionally to the number of consumed source frames. It yields 0 in the
                    // beginning and then grows fast at the end.
                    // However, we also can't pass anti-proportionally adjusted consumed source
                    // frames because the time stretcher may consume lots of source frames in
                    // advance. Even those that will end up being spit out stretched in the next
                    // block or the one after that (= input buffering).
                    // Verdict: At the time this request is made, we have nothing which lets us map
                    // the currently consumed block of source frames to a frame in the destination
                    // block. So our best bet is still total_num_frames_written. So better use
                    // resampling if we want to have accurate bar deviation reporting.
                    audio_block_frame_offset: request.info.audio_block_frame_offset
                        + total_num_frames_written,
                    requester: "time-stretcher-audio",
                    note: "Attention: Using serious time stretching. Analysis results usually have a negative offset (due to input buffering).",
                    is_realtime: false
                },
                parent_request: Some(request),
                general_info: &request.general_info,
            };
            let inner_response = self
                .supplier
                .supply_audio(&inner_request, &mut stretch_buffer);
            if inner_response.num_frames_consumed == 0 {
                break true;
            }
            total_num_frames_consumed += inner_response.num_frames_consumed;
            use SupplyResponseStatus::*;
            let num_inner_frames_written = match inner_response.status {
                PleaseContinue => stretch_buffer.frame_count(),
                ReachedEnd { num_frames_written } => num_frames_written,
            };
            api.BufferDone(num_inner_frames_written as _);
            // Get output material.
            let mut offset_buffer = dest_buffer.slice_mut(total_num_frames_written..);
            let num_frames_written = unsafe {
                api.GetSamples(
                    offset_buffer.frame_count() as _,
                    offset_buffer.data_as_mut_ptr(),
                )
            };
            total_num_frames_written += num_frames_written as usize;
            // println!(
            //     "num_frames_read: {}, total_num_frames_read: {}, num_frames_written: {}, total_num_frames_written: {}",
            //     response.num_frames_written, total_num_frames_read, num_frames_written, total_num_frames_written
            // );
            if total_num_frames_written >= dest_buffer.frame_count() {
                // We have enough stretched material.
                break false;
            }
        };
        SupplyResponse {
            num_frames_consumed: total_num_frames_consumed,
            status: if reached_end {
                SupplyResponseStatus::ReachedEnd {
                    num_frames_written: total_num_frames_written,
                }
            } else {
                SupplyResponseStatus::PleaseContinue
            },
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: MidiSupplier> MidiSupplier for TimeStretcher<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        if !self.enabled {
            return self.supplier.supply_midi(&request, event_list);
        }
        let request = SupplyMidiRequest {
            start_frame: request.start_frame,
            dest_frame_count: request.dest_frame_count,
            dest_sample_rate: Hz::new(request.dest_sample_rate.get() / self.tempo_factor),
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "time-stretcher-midi",
                note: "",
                is_realtime: false,
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        self.supplier.supply_midi(&request, event_list)
    }
}

impl<S: PreBufferSourceSkill> PreBufferSourceSkill for TimeStretcher<S> {
    fn pre_buffer(&mut self, request: PreBufferFillRequest) {
        self.supplier.pre_buffer(request);
    }
}

impl<S: WithFrameRate> WithFrameRate for TimeStretcher<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

pub enum StretchWorkerRequest {
    Stretch,
}

/// A function that keeps processing stretch worker requests until the channel of the given receiver
/// is dropped.
// TODO-high Implement this
pub fn keep_stretching(_requests: Receiver<StretchWorkerRequest>) {}
