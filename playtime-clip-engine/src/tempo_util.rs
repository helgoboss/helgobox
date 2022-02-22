use reaper_high::{Project, Reaper};
use reaper_medium::{Bpm, DurationInSeconds, PositionInSeconds};

pub fn detect_tempo(duration: DurationInSeconds, project: Option<Project>) -> Bpm {
    const MIN_BPM: f64 = 80.0;
    const MAX_BPM: f64 = 200.0;
    let project = project.unwrap_or_else(|| Reaper::get().current_project());
    let result = Reaper::get()
        .medium_reaper()
        .time_map_2_time_to_beats(project.context(), PositionInSeconds::ZERO);
    let numerator = result.time_signature.numerator;
    let mut bpm = numerator.get() as f64 * 60.0 / duration.get();
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
    }
    Bpm::new(bpm)
}
