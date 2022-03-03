use crate::rt::buffer::AudioBufMut;

/// Takes care of applying a fade-in starting at frame zero.
///
/// The portion left of it will be muted.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_in_starting_at_zero(block: &mut AudioBufMut, block_start_frame: isize) {
    use BlockLocation::*;
    match block_location(block_start_frame, block.frame_count()) {
        ContainingFadePortion => {
            block.modify_frames(|sample| {
                let factor =
                    calc_fade_in_volume_factor_at(block_start_frame + sample.index.frame as isize);
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
) {
    let adjusted_block_start_frame =
        block_start_frame - frame_count as isize + FADE_LENGTH as isize;
    apply_fade_out_starting_at_zero(block, adjusted_block_start_frame);
}

/// Takes care of applying a fade-out starting at frame zero.
///
/// The portion right of it will be muted.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_out_starting_at_zero(block: &mut AudioBufMut, block_start_frame: isize) {
    use BlockLocation::*;
    match block_location(block_start_frame, block.frame_count()) {
        ContainingFadePortion => {
            block.modify_frames(|sample| {
                let factor =
                    calc_fade_out_volume_factor_at(block_start_frame + sample.index.frame as isize);
                sample.value * factor
            });
        }
        LeftOfFade => {}
        RightOfFade => {
            block.clear();
        }
    }
}

fn calc_fade_in_volume_factor_at(frame: isize) -> f64 {
    if frame < 0 {
        // Left of fade
        return 0.0;
    }
    if frame >= FADE_LENGTH as isize {
        // Right of fade
        return 1.0;
    }
    frame as f64 / FADE_LENGTH as f64
}

fn calc_fade_out_volume_factor_at(frame: isize) -> f64 {
    if frame < 0 {
        // Left of fade
        return 1.0;
    }
    if frame >= FADE_LENGTH as isize {
        // Right of fade
        return 0.0;
    }
    (frame - FADE_LENGTH as isize).abs() as f64 / FADE_LENGTH as f64
}

fn block_location(block_start_frame: isize, block_frame_count: usize) -> BlockLocation {
    if block_start_frame > FADE_LENGTH as isize {
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

// 480 frames = 10ms at 48 kHz
pub const FADE_LENGTH: usize = 240;
