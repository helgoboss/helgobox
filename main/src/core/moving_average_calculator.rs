#[derive(Default, Debug)]
pub struct MovingAverageCalculator {
    window_size: u64,
    value_count_so_far: u64,
    moving_average: f64,
}

impl MovingAverageCalculator {
    pub fn feed(&mut self, value: f64) {
        self.moving_average = if self.value_count_so_far < self.window_size {
            calc_ma(self.moving_average, self.value_count_so_far, value)
        } else {
            calc_ema(self.moving_average, self.window_size, value)
        };
        self.value_count_so_far += 1;
    }

    pub fn moving_average(&self) -> Option<f64> {
        if self.value_count_so_far < self.window_size {
            return None;
        }
        Some(self.moving_average)
    }

    pub fn value_count_so_far(&self) -> u64 {
        self.value_count_so_far
    }

    pub fn reset(&mut self) {
        self.value_count_so_far = 0;
        self.moving_average = 0.0;
    }
}

fn calc_ma(previous_avg: f64, value_count_so_far: u64, new_value: f64) -> f64 {
    let result = value_count_so_far as f64 * previous_avg + new_value;
    result / (value_count_so_far as f64 + 1.0)
}

fn calc_ema(previous_avg: f64, value_count_so_far: u64, new_value: f64) -> f64 {
    let mult = 2.0 / (value_count_so_far as f64 + 1.0);
    (new_value - previous_avg) * mult + previous_avg
}
