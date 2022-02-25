use playtime_api::TempoRange;
use reaper_high::{Project, Reaper};
use reaper_medium::{Bpm, DurationInSeconds, PositionInSeconds};

pub fn detect_tempo(
    duration: DurationInSeconds,
    project: Project,
    common_tempo_range: TempoRange,
) -> Bpm {
    let result = Reaper::get()
        .medium_reaper()
        .time_map_2_time_to_beats(project.context(), PositionInSeconds::ZERO);
    let numerator = result.time_signature.numerator;
    let mut bpm = numerator.get() as f64 * 60.0 / duration.get();
    while bpm < common_tempo_range.min().get() {
        bpm *= 2.0;
    }
    while bpm > common_tempo_range.max().get() {
        bpm /= 2.0;
    }
    Bpm::new(bpm)
}
