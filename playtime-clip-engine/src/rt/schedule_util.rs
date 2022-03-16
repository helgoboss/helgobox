use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_other_frame_rate,
    convert_position_in_seconds_to_frames,
};
use crate::rt::QuantizedPosCalcEquipment;
use crate::{QuantizedPosition, Timeline};
use reaper_medium::PositionInSeconds;

/// So, this is how we do play scheduling. Whenever the preview register
/// calls get_samples() and we are in a fresh ScheduledOrPlaying state, the
/// relative number of count-in frames will be determined. Based on the given
/// absolute bar for which the clip is scheduled.
///
/// 1. We use a *relative* count-in (instead of just
/// using the absolute scheduled-play position and check if we reached it)
/// in order to respect arbitrary tempo changes during the count-in phase and
/// still end up starting on the correct point in time. Okay, we could reach
/// the same goal also by regularly checking whether we finally reached the
/// start of the bar. But first, we need the relative count-in anyway for pickup beats,
/// which start to play during count-in time. And second, just counting is cheaper
/// than repeatedly doing time/beat mapping.
///
/// 2. We resolve the count-in length here, not at the time the play is requested.
/// Reason: Here we have block information such as block length and frame rate available.
/// That's not an urgent reason ... we could always cache this information and thus make it
/// available in the play request itself. Or we make sure that play/stop is always triggered
/// via receiving in get_samples()! That's good! TODO-medium Implement it.
/// In the past there were more urgent reasons but they are gone. I'll document them here
/// because they might remove doubt in case of possible future refactorings:
///
/// 2a) The play request didn't happen in a real-time thread but in the main thread.
/// At that time it was important to resolve in get_samples() because the start time of the
/// next bar at play-request time was not necessarily the same as the one in the get_samples()
/// call, which would lead to wrong results. However, today, play requests always happen in
/// the real-time thread (a change introduced in favor of a lock-free design).
///
/// 2b) I still thought that it would be better to do it here in case "Live FX multiprocessing"
/// is enabled. If this is enabled, it means get_samples() will in most situations be called in
/// a different real-time thread (some REAPER worker thread) than the play-request code
/// (audio interface thread). I worried that GetPlayPosition2Ex() in the worker thread would
/// return a different position as the audio interface thread would do. However, Justin
/// assured that the worker threads are designed to be synchronous with the audio interface
/// thread and they return the same values. So this is not a reason anymore.
pub fn calc_distance_from_quantized_pos(
    quantized_pos: QuantizedPosition,
    equipment: QuantizedPosCalcEquipment,
) -> isize {
    // Essential calculation
    let quantized_timeline_pos = equipment.timeline.pos_of_quantized_pos(quantized_pos);
    calc_distance_from_pos(quantized_timeline_pos, equipment)
}

pub fn calc_distance_from_pos(
    quantized_timeline_pos: PositionInSeconds,
    equipment: QuantizedPosCalcEquipment,
) -> isize {
    // Essential calculation
    let rel_pos_from_quant_in_secs = equipment.timeline_cursor_pos - quantized_timeline_pos;
    let rel_pos_from_quant_in_source_frames = convert_position_in_seconds_to_frames(
        rel_pos_from_quant_in_secs,
        equipment.source_frame_rate,
    );
    //region Description
    // Now we have a countdown/position in source frames, but it doesn't yet
    // take the tempo adjustment of the source into account.
    // Once we have initialized the countdown with the first value, each
    // get_samples() call - including this one - will advance it by a frame
    // count that ideally = block length in source frames * tempo factor.
    // We use this countdown approach for two reasons.
    //
    // 1. In order to allow tempo changes during count-in time.
    // 2. If the downbeat is > 0, the count-in phase plays source material already.
    //
    // Especially (2) means that the count-in phase will not always have that
    // ideal length which makes the source frame ZERO be perfectly aligned with
    // the ZERO of the timeline bar. I think this is unavoidable when dealing
    // with material that needs sample-rate conversion and/or time
    // stretching. So if one of this is involved, this is just an estimation.
    // However, in real-world scenarios this usually results in slight start
    // deviations around 0-5ms, so it still makes sense musically.
    //endregion
    let block_length_in_source_frames = convert_duration_in_frames_to_other_frame_rate(
        equipment.audio_request_props.block_length,
        equipment.audio_request_props.frame_rate,
        equipment.source_frame_rate,
    );
    adjust_proportionally_in_blocks(
        rel_pos_from_quant_in_source_frames,
        equipment.clip_tempo_factor,
        block_length_in_source_frames,
    )
}

/// It can make a difference if we apply a factor once on a large integer x and then round or
/// n times on x/n and round each time. Latter is what happens in practice because we advance
/// frames step by step in n blocks.
fn adjust_proportionally_in_blocks(value: isize, factor: f64, block_length: usize) -> isize {
    let abs_value = value.abs() as usize;
    let block_count = abs_value / block_length;
    let remainder = abs_value % block_length;
    let adjusted_block_length = adjust_proportionally_positive(block_length as f64, factor);
    let adjusted_remainder = adjust_proportionally_positive(remainder as f64, factor);
    let total_without_remainder = block_count * adjusted_block_length;
    let total = total_without_remainder + adjusted_remainder;
    // dbg!(abs_value, adjusted_block_length, block_count, remainder, adjusted_remainder, total_without_remainder, total);
    total as isize * value.signum()
}
