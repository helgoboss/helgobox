use crate::conversion_util::adjust_proportionally_positive;
use crate::rt::buffer::{AudioBuf, AudioBufMut};
use crate::rt::supplier::log_util::print_distance_from_beat_start_at;
use crate::rt::supplier::{SupplyAudioRequest, SupplyResponse, SupplyResponseStatus};
use reaper_medium::Hz;
use std::cmp;

/// Helper function for suppliers that read from sources and don't want to deal with
/// negative start frames themselves.
pub fn supply_audio_material(
    request: &SupplyAudioRequest,
    dest_buffer: &mut AudioBufMut,
    source_frame_rate: Hz,
    supply_inner: impl FnOnce(SourceMaterialRequest) -> SupplyResponse,
) -> SupplyResponse {
    use SupplyResponseStatus::*;
    #[cfg(debug_assertions)]
    {
        request.assert_wants_source_frame_rate(source_frame_rate);
    }
    let ideal_num_consumed_frames = dest_buffer.frame_count();
    let ideal_end_frame = request.start_frame + ideal_num_consumed_frames as isize;
    if ideal_end_frame <= 0 {
        // Requested portion is located entirely before the actual source material.
        // rt_debug!(
        //     "ideal end frame {} ({})",
        //     ideal_end_frame, ideal_num_consumed_frames
        // );
        dest_buffer.clear();
        // We haven't reached the end of the source, so still tell the caller that we
        // wrote all frames.
        // And advance the count-in phase.
        SupplyResponse::please_continue(ideal_num_consumed_frames)
    } else {
        // Requested portion contains playable material.
        if request.start_frame < 0 {
            // Portion overlaps start of material.
            // rt_debug!(
            //     "overlap: start_frame = {}, ideal_end_frame = {}",
            //     request.start_frame, ideal_end_frame
            // );
            let num_skipped_frames_in_source = -request.start_frame as usize;
            let proportion_skipped =
                num_skipped_frames_in_source as f64 / ideal_num_consumed_frames as f64;
            let num_skipped_frames_in_dest = adjust_proportionally_positive(
                dest_buffer.frame_count() as f64,
                proportion_skipped,
            );
            // We need to zero the portion of the output buffer that precedes start of material.
            dest_buffer.slice_mut(..num_skipped_frames_in_dest).clear();
            if request.info.is_realtime {
                print_distance_from_beat_start_at(
                    request,
                    num_skipped_frames_in_dest,
                    "audio, start_frame < 0",
                );
            }
            let mut shifted_dest_buffer = dest_buffer.slice_mut(num_skipped_frames_in_dest..);
            let req = SourceMaterialRequest {
                start_frame: 0,
                dest_buffer: &mut shifted_dest_buffer,
            };
            // rt_debug!(
            //     "Before source: start = {}, source sr = {}, dest sr = {}",
            //     req.start_frame, req.source_sample_rate, req.dest_sample_rate
            // );
            let res = supply_inner(req);
            SupplyResponse {
                num_frames_consumed: num_skipped_frames_in_source + res.num_frames_consumed,
                status: match res.status {
                    PleaseContinue => PleaseContinue,
                    ReachedEnd { num_frames_written } => {
                        // Oh, that's short material.
                        shifted_dest_buffer.slice_mut(num_frames_written..).clear();
                        ReachedEnd {
                            num_frames_written: num_skipped_frames_in_dest + num_frames_written,
                        }
                    }
                },
            }
        } else {
            // Requested portion is located on or after start of the actual source material.
            if request.start_frame == 0 && request.info.is_realtime {
                print_distance_from_beat_start_at(request, 0, "audio, start_frame == 0");
            }
            let req = SourceMaterialRequest {
                start_frame: request.start_frame as usize,
                dest_buffer,
            };
            // rt_debug!(
            //     "In source: start = {}, source sr = {}, dest sr = {}",
            //     req.start_frame, req.source_sample_rate, req.dest_sample_rate
            // );
            let inner_response = supply_inner(req);
            // Because neither resampler nor time stretcher pre-zero buffers, we need to zero
            // the unwritten part of the buffer ourselves, otherwise we get some nice saw-like
            // tone at the end. This is particularly audible if we have a section that exceeds
            // the source end. Then the last buffer content will get played repeatedly.
            if let ReachedEnd { num_frames_written } = inner_response.status {
                dest_buffer.slice_mut(num_frames_written..).clear();
            }
            inner_response
        }
    }
}

pub struct SourceMaterialRequest<'a, 'b> {
    pub start_frame: usize,
    pub dest_buffer: &'a mut AudioBufMut<'b>,
}

pub fn transfer_samples_from_buffer(buf: AudioBuf, req: SourceMaterialRequest) -> SupplyResponse {
    let num_remaining_frames_in_source = buf.frame_count() - req.start_frame;
    let num_frames_written = cmp::min(
        num_remaining_frames_in_source,
        req.dest_buffer.frame_count(),
    );
    if num_frames_written == 0 {
        return SupplyResponse::exceeded_end();
    }
    let end_frame = req.start_frame + num_frames_written;
    buf.slice(req.start_frame..end_frame)
        .copy_to(&mut req.dest_buffer.slice_mut(0..num_frames_written));
    SupplyResponse::limited_by_total_frame_count(
        num_frames_written,
        num_frames_written,
        req.start_frame as isize,
        buf.frame_count(),
    )
}
