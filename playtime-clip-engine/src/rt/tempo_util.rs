use crate::rt::supplier::MIDI_BASE_BPM;
use playtime_api::{BeatTimeBase, ClipTimeBase, TempoRange};
use reaper_high::{Project, Reaper};
use reaper_medium::{Bpm, DurationInSeconds, PositionInSeconds};

#[allow(dead_code)]
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

/// Returns `None` if time base is not "Beat".
pub fn determine_tempo_from_time_base(time_base: &ClipTimeBase, is_midi: bool) -> Option<Bpm> {
    use ClipTimeBase::*;
    match time_base {
        Time => None,
        Beat(b) => Some(determine_tempo_from_beat_time_base(b, is_midi)),
    }
}

pub fn determine_tempo_from_beat_time_base(beat_time_base: &BeatTimeBase, is_midi: bool) -> Bpm {
    if is_midi {
        MIDI_BASE_BPM
    } else {
        let tempo = beat_time_base
            .audio_tempo
            .expect("material has time base 'beat' but no tempo");
        Bpm::new(tempo.get())
    }
}

pub fn calc_tempo_factor(clip_tempo: Bpm, timeline_tempo: Bpm) -> f64 {
    let timeline_tempo_factor = timeline_tempo.get() / clip_tempo.get();
    timeline_tempo_factor.max(MIN_TEMPO_FACTOR)
}

const MIN_TEMPO_FACTOR: f64 = 0.0000000001;
