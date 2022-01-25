use crate::domain::clip_engine::buffer::AudioBufMut;
use crate::domain::clip_engine::supplier::{
    AudioSupplier, Ctx, ExactFrameCount, SupplyAudioRequest, SupplyResponse, WithFrameRate,
};
use crate::domain::clip_engine::{adjust_proportionally_positive, SupplyRequestInfo};
use reaper_high::Reaper;
use reaper_low::raw::REAPER_Resample_Interface;
use reaper_medium::Hz;
use std::ptr::null_mut;

#[derive(Debug)]
pub struct Resampler {
    // TODO-high Only static until we have a proper owned version (destruction!)
    api: &'static REAPER_Resample_Interface,
}

impl Resampler {
    pub fn new() -> Self {
        let api = Reaper::get().medium_reaper().low().Resampler_Create();
        let api = unsafe { &*api };
        Self { api }
    }

    pub fn reset(&mut self) {
        self.api.Reset();
    }
}

impl<'a, S: AudioSupplier + WithFrameRate> AudioSupplier for Ctx<'a, Resampler, S> {
    fn supply_audio(
        &self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let mut total_num_frames_consumed = 0usize;
        let mut total_num_frames_written = 0usize;
        let source_frame_rate = self.supplier.frame_rate();
        let source_channel_count = self.supplier.channel_count();
        let dest_sample_rate = Hz::new(request.dest_sample_rate.get() / self.tempo_factor);
        self.mode
            .api
            .SetRates(source_frame_rate.get(), dest_sample_rate.get());
        // Set ResamplePrepare's out_samples to refer to request a specific number of input samples.
        // const RESAMPLE_EXT_SETFEEDMODE: i32 = 0x1001;
        // let ext_result = unsafe {
        //     self.mode.api.Extended(
        //         RESAMPLE_EXT_SETFEEDMODE,
        //         1 as *mut _,
        //         null_mut(),
        //         null_mut(),
        //     )
        // };
        loop {
            // Get resampler buffer.
            let buffer_frame_count = 128usize;
            let mut resample_buffer: *mut f64 = null_mut();
            let num_source_frames_to_write = unsafe {
                self.mode.api.ResamplePrepare(
                    buffer_frame_count as _,
                    source_channel_count as i32,
                    &mut resample_buffer,
                )
            };
            let mut resample_buffer = unsafe {
                AudioBufMut::from_raw(
                    resample_buffer,
                    source_channel_count,
                    num_source_frames_to_write as _,
                )
            };
            // Feed resampler buffer with source material.
            let inner_request = SupplyAudioRequest {
                start_frame: request.start_frame + total_num_frames_consumed as isize,
                dest_sample_rate: source_frame_rate,
                info: SupplyRequestInfo {
                    audio_block_frame_offset: request.info.audio_block_frame_offset
                        + total_num_frames_written,
                    requester: "active-resampler",
                    note: "",
                },
                parent_request: Some(request),
                general_info: request.general_info,
            };
            let inner_response = self
                .supplier
                .supply_audio(&inner_request, &mut resample_buffer);
            total_num_frames_consumed += inner_response.num_frames_written;
            assert_eq!(
                inner_response.num_frames_written,
                inner_response.num_frames_consumed
            );
            // Get output material.
            let mut offset_buffer = dest_buffer.slice_mut(total_num_frames_written..);
            let num_frames_written = unsafe {
                self.mode.api.ResampleOut(
                    offset_buffer.data_as_mut_ptr(),
                    num_source_frames_to_write,
                    offset_buffer.frame_count() as _,
                    dest_buffer.channel_count() as _,
                )
            };
            total_num_frames_written += num_frames_written as usize;
            if total_num_frames_written >= dest_buffer.frame_count() {
                // We have enough resampled material.
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
        // // TODO-high At lower sample rates there are sometimes clicks. Rounding errors?
        // let request = SupplyAudioRequest {
        //     start_frame: request.start_frame,
        //     dest_sample_rate: Hz::new(request.dest_sample_rate.get() / self.tempo_factor),
        //     info: SupplyRequestInfo {
        //         audio_block_frame_offset: request.info.audio_block_frame_offset,
        //         requester: "resampler",
        //         note: "",
        //     },
        //     parent_request: Some(request),
        //     general_info: request.general_info,
        // };
        // self.supplier.supply_audio(&request, dest_buffer)
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<'a, S: WithFrameRate> WithFrameRate for Ctx<'a, Resampler, S> {
    fn frame_rate(&self) -> Hz {
        self.supplier.frame_rate()
    }
}
