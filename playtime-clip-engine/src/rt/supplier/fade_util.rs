use crate::rt::buffer::AudioBufMut;

/// Takes care of applying a fade-in to the first few frames of a larger audio portion.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_in(block: &mut AudioBufMut, block_start_frame: isize) {
    if block_start_frame > FADE_LENGTH as isize {
        // Block is right of fade
        return;
    }
    let block_end_frame = block_start_frame + block.frame_count() as isize;
    if block_end_frame < 0 {
        // Block is left of fade
        return;
    }
    // Block contains at least a portion of the fade
    block.modify_frames(|frame, sample| {
        let factor = calc_fade_in_volume_factor_at(block_start_frame + frame as isize);
        sample * factor
    });
}

/// Takes care of applying a fade-out to the last few frames of a larger audio portion.
///
/// The `block_start_frame` parameter indicates the position of the block within the larger audio
/// portion.
pub fn apply_fade_out(block: &mut AudioBufMut, block_start_frame: isize, frame_count: usize) {
    if block_start_frame > frame_count as isize {
        // Block is right of fade
        return;
    }
    let block_end_frame = block_start_frame + block.frame_count() as isize;
    let fade_start_frame = frame_count as isize - FADE_LENGTH as isize;
    if block_end_frame < fade_start_frame {
        // Block is left of fade
        return;
    }
    // Block contains at least a portion of the fade
    block.modify_frames(|frame, sample| {
        let factor =
            calc_fade_out_volume_factor_at(block_start_frame + frame as isize, frame_count);
        sample * factor
    });
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

fn calc_fade_out_volume_factor_at(frame: isize, frame_count: usize) -> f64 {
    if frame > frame_count as isize {
        // Right of fade
        return 0.0;
    }
    let distance_to_end = frame_count as isize - frame;
    if distance_to_end > FADE_LENGTH as isize {
        // Left of fade
        return 1.0;
    }
    distance_to_end as f64 / FADE_LENGTH as f64
}

// 480 frames = 10ms at 48 kHz
const FADE_LENGTH: usize = 48000;
