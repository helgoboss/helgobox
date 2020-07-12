use crate::core::MovingAverageCalculator;
use helgoboss_learn::{Bpm, MidiSourceValue};
use helgoboss_midi::RawShortMessage;
use reaper_medium::{Hz, MidiFrameOffset};

pub struct MidiClockCalculator {
    sample_rate: Hz,
    sample_counter: u64,
    previous_midi_clock_timestamp_in_samples: u64,
    bpm_calculator: MovingAverageCalculator,
}

impl Default for MidiClockCalculator {
    fn default() -> Self {
        Self {
            sample_rate: Hz::new(1.0),
            sample_counter: 0,
            previous_midi_clock_timestamp_in_samples: 0,
            bpm_calculator: Default::default(),
        }
    }
}

impl MidiClockCalculator {
    pub fn update_sample_rate(&mut self, sample_rate: Hz) {
        self.sample_rate = sample_rate;
    }

    pub fn increase_sample_counter_by(&mut self, sample_count: u64) {
        self.sample_counter += sample_count as u64;
    }

    pub fn feed(&mut self, frame_offset: MidiFrameOffset) -> Option<Bpm> {
        // Frame offset is given in 1/1024000 of a second, *not* sample frames!
        let offset_in_secs = frame_offset.get() as f64 / 1024000.0;
        let offset_in_samples = (offset_in_secs * self.sample_rate.get()).round() as u64;
        let timestamp_in_samples = self.sample_counter + offset_in_samples;
        let prev_timestamp = self.previous_midi_clock_timestamp_in_samples;
        self.previous_midi_clock_timestamp_in_samples = timestamp_in_samples;

        if prev_timestamp == 0 || timestamp_in_samples <= prev_timestamp {
            return None;
        }
        let difference_in_samples = timestamp_in_samples - prev_timestamp;
        let difference_in_secs = difference_in_samples as f64 / self.sample_rate.get();
        let num_ticks_per_sec = 1.0 / difference_in_secs;
        let num_beats_per_sec = num_ticks_per_sec / 24.0;
        let num_beats_per_min = num_beats_per_sec * 60.0;
        if num_beats_per_min > 300.0 {
            return None;
        }
        self.bpm_calculator.feed(num_beats_per_min);
        let moving_avg = match self.bpm_calculator.moving_average() {
            None => return None,
            Some(a) => a,
        };
        if self.bpm_calculator.value_count_so_far() % 24 == 0 {
            Some(Bpm::new(moving_avg))
        } else {
            None
        }
    }

    pub fn current_sample_count(&self) -> u64 {
        self.sample_counter
    }
}
