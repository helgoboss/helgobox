use crate::conversion_util::{
    convert_duration_in_frames_to_seconds, convert_position_in_seconds_to_frames,
};
use crate::rt::supplier::SupplyRequest;
use crate::timeline::{clip_timeline, Timeline};
use crate::{Laziness, QuantizedPosition};
use playtime_api::persistence::EvenQuantization;
use reaper_medium::PositionInSeconds;
use std::cmp;

/// This deals with timeline units only.
pub fn print_distance_from_beat_start_at(
    request: &impl SupplyRequest,
    additional_block_offset: usize,
    comment: &str,
) {
    let effective_block_offset = request.info().audio_block_frame_offset + additional_block_offset;
    let offset_in_timeline_secs = convert_duration_in_frames_to_seconds(
        effective_block_offset,
        request.general_info().output_frame_rate,
    );
    let ref_pos = request.general_info().audio_block_timeline_cursor_pos + offset_in_timeline_secs;
    let timeline = clip_timeline(None, false);
    let next_bar = timeline
        .next_quantized_pos_at(
            ref_pos,
            EvenQuantization::ONE_BAR,
            Laziness::DwellingOnCurrentPos,
        )
        .position() as i32;
    struct BarInfo {
        bar: i32,
        pos: PositionInSeconds,
        rel_pos: PositionInSeconds,
    }
    let create_bar_info = |bar| {
        let bar_pos = timeline.pos_of_quantized_pos(QuantizedPosition::bar(bar as i64));
        BarInfo {
            bar,
            pos: bar_pos,
            rel_pos: ref_pos - bar_pos,
        }
    };
    let current_bar_info = create_bar_info(next_bar - 1);
    let next_bar_info = create_bar_info(next_bar);
    let closest = cmp::min_by_key(&current_bar_info, &next_bar_info, |v| v.rel_pos.abs());
    let rel_pos_from_closest_bar_in_timeline_frames = convert_position_in_seconds_to_frames(
        closest.rel_pos,
        request.general_info().output_frame_rate,
    );
    let block_duration = convert_duration_in_frames_to_seconds(
        request.general_info().audio_block_length,
        request.general_info().output_frame_rate,
    );
    let block_index = (request.general_info().audio_block_timeline_cursor_pos.get()
        / block_duration.get()) as isize;
    debug!(
        "\n\
        # New loop cycle\n\
        Block index: {}\n\
        Block start position: {:.3}s\n\
        Closest bar: {}\n\
        Closest bar timeline position: {:.3}s\n\
        Relative position from closest bar: {:.3}ms (= {} timeline frames)\n\
        Effective block offset: {},\n\
        Requester: {}\n\
        Note: {}\n\
        Comment: {}\n\
        Clip tempo factor: {}\n\
        Timeline tempo: {}\n\
        Parent requester: {:?}\n\
        Parent note: {:?}\n\
        ",
        block_index,
        request.general_info().audio_block_timeline_cursor_pos,
        closest.bar,
        closest.pos.get(),
        closest.rel_pos.get() * 1000.0,
        rel_pos_from_closest_bar_in_timeline_frames,
        effective_block_offset,
        request.info().requester,
        request.info().note,
        comment,
        request.general_info().clip_tempo_factor,
        request.general_info().timeline_tempo,
        request.parent_request().map(|r| r.info().requester),
        request.parent_request().map(|r| r.info().note)
    );
}
