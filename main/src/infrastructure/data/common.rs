use helgoboss_learn::{Interval, DEFAULT_OSC_ARG_VALUE_RANGE};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntervalData<T> {
    min: T,
    max: T,
}

impl<T: Copy + PartialOrd> IntervalData<T> {
    pub fn from_interval(interval: Interval<T>) -> Self {
        Self {
            min: interval.min_val(),
            max: interval.max_val(),
        }
    }

    pub fn to_interval(&self) -> Interval<T> {
        Interval::new_auto(self.min, self.max)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OscValueRange(IntervalData<f64>);

impl Default for OscValueRange {
    fn default() -> Self {
        Self(IntervalData::from_interval(DEFAULT_OSC_ARG_VALUE_RANGE))
    }
}

impl OscValueRange {
    pub fn from_interval(interval: Interval<f64>) -> Self {
        Self(IntervalData::from_interval(interval))
    }

    pub fn to_interval(&self) -> Interval<f64> {
        self.0.to_interval()
    }
}
