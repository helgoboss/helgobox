use crate::domain::ControlEventTimestamp;
use reaper_common_types::Bpm;
use simple_moving_average::{SumTreeSMA, SMA};
use std::convert::TryInto;

#[derive(Debug)]
pub struct MidiClockCalculator {
    previous_timestamp: Option<ControlEventTimestamp>,
    moving_avg_calculator: SumTreeSMA<f64, f64, 10>,
}

impl Default for MidiClockCalculator {
    fn default() -> Self {
        Self {
            previous_timestamp: None,
            moving_avg_calculator: SumTreeSMA::new(),
        }
    }
}

impl MidiClockCalculator {
    pub fn feed(&mut self, timestamp: ControlEventTimestamp) -> Option<Bpm> {
        let prev_timestamp = self.previous_timestamp.replace(timestamp)?;
        let duration_since_last = timestamp - prev_timestamp;
        let num_ticks_per_sec = 1.0 / duration_since_last.as_secs_f64();
        let num_beats_per_sec = num_ticks_per_sec / 24.0;
        let new_bpm = num_beats_per_sec * 60.0;
        self.moving_avg_calculator.add_sample(new_bpm);
        let avg_bpm = self.moving_avg_calculator.get_average();
        let avg_bpm: Bpm = avg_bpm.try_into().ok()?;
        Some(avg_bpm)
    }
}
