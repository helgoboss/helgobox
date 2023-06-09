use crate::rt::buffer::AudioBufMut;

/// Takes care of applying a fade-in starting at frame zero.
///
/// The portion left of it will be muted.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_in_starting_at_zero(
    block: &mut AudioBufMut,
    block_start_frame: isize,
    fade_length: usize,
) {
    use BlockLocation::*;
    match block_location(block_start_frame, block.frame_count(), fade_length) {
        ContainingFadePortion => {
            block.modify_frames(|sample| {
                let factor = calc_fade_in_volume_factor_at(
                    block_start_frame + sample.index.frame as isize,
                    fade_length,
                );
                sample.value * factor
            });
        }
        LeftOfFade => {
            block.clear();
        }
        RightOfFade => {}
    }
}

/// Takes care of applying a fade-out to the last few frames of a larger audio portion.
///
/// The portion right of it will be muted.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_out_ending_at(
    block: &mut AudioBufMut,
    block_start_frame: isize,
    frame_count: usize,
    fade_length: usize,
) {
    let adjusted_block_start_frame =
        block_start_frame - frame_count as isize + fade_length as isize;
    apply_fade_out_starting_at_zero(block, adjusted_block_start_frame, fade_length);
}

/// Takes care of applying a fade-out starting at frame zero.
///
/// The portion right of it will be muted.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_out_starting_at_zero(
    block: &mut AudioBufMut,
    block_start_frame: isize,
    fade_length: usize,
) {
    use BlockLocation::*;
    match block_location(block_start_frame, block.frame_count(), fade_length) {
        ContainingFadePortion => {
            block.modify_frames(|sample| {
                let factor = calc_fade_out_volume_factor_at(
                    block_start_frame + sample.index.frame as isize,
                    fade_length,
                );
                sample.value * factor
            });
        }
        LeftOfFade => {}
        RightOfFade => {
            block.clear();
        }
    }
}

fn calc_fade_in_volume_factor_at(frame: isize, fade_length: usize) -> f64 {
    if frame < 0 {
        // Left of fade
        return 0.0;
    }
    if frame >= fade_length as isize {
        // Right of fade
        return 1.0;
    }
    frame as f64 / fade_length as f64
}

fn calc_fade_out_volume_factor_at(frame: isize, fade_length: usize) -> f64 {
    if frame < 0 {
        // Left of fade
        return 1.0;
    }
    if frame >= fade_length as isize {
        // Right of fade
        return 0.0;
    }
    (frame - fade_length as isize).abs() as f64 / fade_length as f64
}

fn block_location(
    block_start_frame: isize,
    block_frame_count: usize,
    fade_length: usize,
) -> BlockLocation {
    if block_start_frame > fade_length as isize {
        return BlockLocation::RightOfFade;
    }
    let block_end_frame = block_start_frame + block_frame_count as isize;
    if block_end_frame < 0 {
        return BlockLocation::LeftOfFade;
    }
    BlockLocation::ContainingFadePortion
}

enum BlockLocation {
    ContainingFadePortion,
    LeftOfFade,
    RightOfFade,
}

// 240 frames = 5ms at 48 kHz
// TODO-high-clip-engine That's not enough. Take some pad/organ sound and it will click!
//  And this, gentlemen, is why the stop process needs to be asynchronous = needs to cover
//  multiple audio callback cycles.
const FADE_LENGTH: usize = 240;
pub const SECTION_FADE_LENGTH: usize = FADE_LENGTH;
pub const INTERACTION_FADE_LENGTH: usize = FADE_LENGTH;
pub const START_END_FADE_LENGTH: usize = FADE_LENGTH;
