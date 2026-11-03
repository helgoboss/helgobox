//! The code in this file is largely LLM-generated.
//!
//! Non-critical because it's only used during input detection, and it appears to work well enough.

use reaper_low::raw::audio_hook_register_t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnedAudioInput {
    Mono(usize),
    Stereo(usize), // Starting channel, 0-based
}

impl LearnedAudioInput {
    pub fn rec_input(self) -> i32 {
        match self {
            Self::Mono(channel) => channel as i32,
            Self::Stereo(channel) => 1024 + channel as i32,
        }
    }
}

#[derive(Debug)]
pub struct AudioInputLearner {
    channels: Vec<ChannelState>,
    phase: Phase,
    confirmation_samples: u64,
    threshold_squared: f64,
    baseline_factor_squared: f64,
    winner_factor_squared: f64,
    stereo_balance_factor_squared: f64,
}

pub struct AudioInputLearnerConfig {
    /// Duration of the initial noise-floor measurement, in seconds.
    pub baseline_duration: f64,
    /// Duration used to compare candidate channels, in seconds.
    pub confirmation_duration: f64,
    /// Minimum absolute signal level in dBFS.
    pub threshold_db: f64,
    /// Required increase above the baseline, in dB.
    pub baseline_margin_db: f64,
    /// Required difference between winner and runner-up, in dB.
    pub winner_margin_db: f64,
    /// Maximum permitted difference between stereo channels, in dB.
    pub stereo_balance_db: f64,
}

impl Default for AudioInputLearnerConfig {
    fn default() -> Self {
        Self {
            baseline_duration: 0.2,
            confirmation_duration: 0.15,
            threshold_db: -40.0,
            baseline_margin_db: 12.0,
            winner_margin_db: 6.0,
            stereo_balance_db: 12.0,
        }
    }
}

#[derive(Debug, Default)]
struct ChannelState {
    baseline_sum_squares: f64,
    baseline_samples: u64,
    baseline_mean_square: f64,
    confirmation_sum_squares: f64,
    confirmation_samples: u64,
}

#[derive(Debug)]
enum Phase {
    Baseline { remaining_samples: u64 },
    Learning,
    Confirmation { remaining_samples: u64 },
}

impl AudioInputLearner {
    pub fn new(input_channels: usize, sample_rate: f64, config: AudioInputLearnerConfig) -> Self {
        let baseline_samples = (sample_rate * config.baseline_duration).max(1.0) as u64;

        let confirmation_samples = (sample_rate * config.confirmation_duration).max(1.0) as u64;

        Self {
            channels: (0..input_channels)
                .map(|_| ChannelState::default())
                .collect(),
            phase: Phase::Baseline {
                remaining_samples: baseline_samples,
            },
            confirmation_samples,
            threshold_squared: db_to_power(config.threshold_db),
            baseline_factor_squared: db_to_power(config.baseline_margin_db),
            winner_factor_squared: db_to_power(config.winner_margin_db),
            stereo_balance_factor_squared: db_to_power(-config.stereo_balance_db),
        }
    }

    /// Feeds an audio buffer and returns the detected input, if any.
    ///
    /// Must be called from the audio thread.
    ///
    /// The number of input channels must not exceed the number configured
    /// at construction. Additional channels are ignored.
    pub fn feed(&mut self, reg: &audio_hook_register_t, len: usize) -> Option<LearnedAudioInput> {
        if len == 0 {
            return None;
        }

        let get_buffer = reg.GetBuffer?;
        let channel_count = self.channels.len();

        let is_baseline = matches!(self.phase, Phase::Baseline { .. });
        let is_learning = matches!(self.phase, Phase::Learning);

        let mut activity_detected = false;

        // Process all channels using the same phase.
        for channel in 0..channel_count {
            let ptr = unsafe { get_buffer(false, channel as i32) };
            if ptr.is_null() {
                continue;
            }

            let samples = unsafe { std::slice::from_raw_parts(ptr, len) };

            let sum_squares: f64 = samples.iter().map(|&s| s * s).sum();
            let mean_square = sum_squares / len as f64;

            let state = &mut self.channels[channel];

            if is_baseline {
                state.baseline_sum_squares += sum_squares;
                state.baseline_samples += len as u64;
            } else {
                if is_learning
                    && mean_square >= self.threshold_squared
                    && mean_square >= state.baseline_mean_square * self.baseline_factor_squared
                {
                    activity_detected = true;
                }

                state.confirmation_sum_squares += sum_squares;
                state.confirmation_samples += len as u64;
            }
        }

        // Update phase only after all channels have been processed.
        match &mut self.phase {
            Phase::Baseline { remaining_samples } => {
                *remaining_samples = remaining_samples.saturating_sub(len as u64);

                if *remaining_samples == 0 {
                    for state in &mut self.channels {
                        if state.baseline_samples > 0 {
                            state.baseline_mean_square =
                                state.baseline_sum_squares / state.baseline_samples as f64;
                        }
                    }

                    self.phase = Phase::Learning;
                }
            }

            Phase::Learning => {
                if activity_detected {
                    self.phase = Phase::Confirmation {
                        remaining_samples: self.confirmation_samples.saturating_sub(len as u64),
                    };
                } else {
                    self.reset_confirmation();
                }
            }

            Phase::Confirmation { remaining_samples } => {
                *remaining_samples = remaining_samples.saturating_sub(len as u64);
            }
        }

        if matches!(
            self.phase,
            Phase::Confirmation {
                remaining_samples: 0
            }
        ) {
            let result = self.evaluate_confirmation();
            self.reset_confirmation();
            self.phase = Phase::Learning;
            return result;
        }

        None
    }

    fn evaluate_confirmation(&self) -> Option<LearnedAudioInput> {
        let mut winner: Option<(LearnedAudioInput, f64)> = None;
        let mut runner_up = 0.0_f64;

        let mut channel = 0;

        while channel < self.channels.len() {
            let left = &self.channels[channel];
            let left_power = self.channel_power(left);

            // Consider conventional stereo pairs (0/1, 2/3, ...).
            if channel % 2 == 0 && channel + 1 < self.channels.len() {
                let right = &self.channels[channel + 1];
                let right_power = self.channel_power(right);

                if self.is_active(left, left_power)
                    && self.is_active(right, right_power)
                    && left_power.min(right_power)
                        >= left_power.max(right_power) * self.stereo_balance_factor_squared
                {
                    // Average power of both stereo channels.
                    let stereo_power = (left_power + right_power) / 2.0;

                    Self::consider_candidate(
                        &mut winner,
                        &mut runner_up,
                        LearnedAudioInput::Stereo(channel),
                        stereo_power,
                    );

                    channel += 2;
                    continue;
                }
            }

            if self.is_active(left, left_power) {
                Self::consider_candidate(
                    &mut winner,
                    &mut runner_up,
                    LearnedAudioInput::Mono(channel),
                    left_power,
                );
            }

            channel += 1;
        }

        let (input, power) = winner?;

        if power < runner_up * self.winner_factor_squared {
            return None;
        }

        Some(input)
    }

    fn channel_power(&self, state: &ChannelState) -> f64 {
        if state.confirmation_samples == 0 {
            return 0.0;
        }

        state.confirmation_sum_squares / state.confirmation_samples as f64
    }

    fn is_active(&self, state: &ChannelState, power: f64) -> bool {
        state.confirmation_samples > 0
            && power >= self.threshold_squared
            && power >= state.baseline_mean_square * self.baseline_factor_squared
    }

    fn consider_candidate(
        winner: &mut Option<(LearnedAudioInput, f64)>,
        runner_up: &mut f64,
        input: LearnedAudioInput,
        power: f64,
    ) {
        if winner.is_none_or(|(_, best)| power > best) {
            if let Some((_, previous)) = *winner {
                *runner_up = runner_up.max(previous);
            }
            *winner = Some((input, power));
        } else {
            *runner_up = runner_up.max(power);
        }
    }

    fn reset_confirmation(&mut self) {
        for state in &mut self.channels {
            state.confirmation_sum_squares = 0.0;
            state.confirmation_samples = 0;
        }
    }
}

fn db_to_power(db: f64) -> f64 {
    10.0_f64.powf(db / 10.0)
}
