use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::supplier::{
    AudioSupplier, Ctx, SupplyAudioRequest, SupplyResponse, WithFrameRate,
};
use crate::domain::clip_engine::{
    adjust_anti_proportionally_positive, adjust_proportionally_positive, SupplyRequestInfo,
};
use crossbeam_channel::Receiver;
use reaper_high::Reaper;
use reaper_low::raw::{IReaperPitchShift, REAPER_PITCHSHIFT_API_VER};
use reaper_medium::Hz;

#[derive(Debug)]
pub struct SeriousTimeStretcher {
    // TODO-high Only static until we have a proper owned version (destruction!)
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
        let mut total_num_frames_consumed = 0usize;
        let mut total_num_frames_written = 0usize;
        let source_frame_rate = self.supplier.frame_rate();
        // I think it makes sense to set both the output and the input sample rate to the sample
        // rate of the source. Then the result can be even cached and sample rate & play-rate
        // changes don't need to invalidate the cache.
        // TODO-high However, we need to add a resampler on top. We could actually always add a
        //  resampler before the time stretcher and just let it do the usual sample rate conversion,
        //  unless the stretch mode is Resampler in which case it should consider the tempo when
        //  when calculating the output sample rate.
        // TODO-medium Setting this right at the beginning should be enough.
        self.mode.api.set_srate(source_frame_rate.get());
        let dest_nch = dest_buffer.channel_count();
        self.mode.api.set_nch(dest_nch as _);
        self.mode.api.set_tempo(self.tempo_factor);
        loop {
            // Get time stretcher buffer.
            let buffer_frame_count = 128usize;
            let stretch_buffer = self.mode.api.GetBuffer(buffer_frame_count as _);
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
                    requester: "time-stretcher",
                    note: "Attention: Using serious time stretching. Analysis results usually have a negative offset (due to input buffering)."
                },
                parent_request: Some(request),
                general_info: &request.general_info,
            };
            let inner_response = self
                .supplier
                .supply_audio(&inner_request, &mut stretch_buffer);
            total_num_frames_consumed += inner_response.num_frames_written;
            assert_eq!(
                inner_response.num_frames_written,
                inner_response.num_frames_consumed
            );
            self.mode
                .api
                .BufferDone(inner_response.num_frames_written as _);
            // Get output material.
            let mut offset_buffer = dest_buffer.slice_mut(total_num_frames_written..);
            let num_frames_written = unsafe {
                self.mode.api.GetSamples(
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
                break;
            }
        }
        assert_eq!(
            total_num_frames_written,
            dest_buffer.frame_count(),
            "wrote more frames than requested"
        );
        let next_frame = request.start_frame + total_num_frames_consumed as isize;
        SupplyResponse {
            num_frames_written: total_num_frames_written,
            num_frames_consumed: total_num_frames_consumed,
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
