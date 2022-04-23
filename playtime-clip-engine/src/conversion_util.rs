use reaper_medium::{DurationInSeconds, Hz, PositionInSeconds};

pub fn convert_duration_in_seconds_to_frames(seconds: DurationInSeconds, sample_rate: Hz) -> usize {
    (seconds.get() * sample_rate.get()).round() as usize
}

pub fn convert_position_in_seconds_to_frames(seconds: PositionInSeconds, sample_rate: Hz) -> isize {
    (seconds.get() * sample_rate.get()).round() as isize
}

pub fn adjust_proportionally_positive(frame_count: f64, factor: f64) -> usize {
    adjust_proportionally(frame_count, factor) as usize
}

pub fn adjust_proportionally(frame_count: f64, factor: f64) -> isize {
    (frame_count as f64 * factor).round() as isize
}

pub fn adjust_pos_in_secs_anti_proportionally(
    pos: PositionInSeconds,
    factor: f64,
) -> PositionInSeconds {
    PositionInSeconds::new(pos.get() / factor)
}

pub fn adjust_duration_in_secs_anti_proportionally(
    pos: DurationInSeconds,
    factor: f64,
) -> DurationInSeconds {
    DurationInSeconds::new(pos.get() / factor)
}

pub fn adjust_duration_in_secs_proportionally(
    pos: DurationInSeconds,
    factor: f64,
) -> DurationInSeconds {
    DurationInSeconds::new(pos.get() * factor)
}

pub fn convert_duration_in_frames_to_seconds(
    frame_count: usize,
    sample_rate: Hz,
) -> DurationInSeconds {
    DurationInSeconds::new(frame_count as f64 / sample_rate.get())
}

pub fn convert_position_in_frames_to_seconds(
    frame_count: isize,
    sample_rate: Hz,
) -> PositionInSeconds {
    PositionInSeconds::new(frame_count as f64 / sample_rate.get())
}

pub fn convert_duration_in_frames_to_other_frame_rate(
    frame_count: usize,
    in_sample_rate: Hz,
    out_sample_rate: Hz,
) -> usize {
    let ratio = out_sample_rate.get() / in_sample_rate.get();
    (ratio * frame_count as f64).round() as usize
}
