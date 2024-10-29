use crate::domain::GLOBAL_AUDIO_STATE;
use helgoboss_learn::AbstractTimestamp;
use reaper_common_types::{DurationInSeconds, Hz};
use std::fmt::{Display, Formatter};
use std::ops::Sub;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub type ControlEvent<P> = helgoboss_learn::ControlEvent<P, ControlEventTimestamp>;

/// Timestamp of a control event.
//
// Don't expose the inner field, it should stay private. We might swap the time unit in future to
// improve performance and accuracy.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ControlEventTimestamp(Duration);

impl ControlEventTimestamp {
    pub fn from_main_thread() -> Self {
        let block_count = GLOBAL_AUDIO_STATE.load_block_count();
        let block_size = GLOBAL_AUDIO_STATE.load_block_size();
        let sample_count = block_count * block_size as u64;
        Self::from_rt(
            sample_count,
            GLOBAL_AUDIO_STATE.load_sample_rate(),
            DurationInSeconds::ZERO,
        )
    }
}

impl AbstractTimestamp for ControlEventTimestamp {
    fn duration(&self) -> Duration {
        self.0
    }
}

impl ControlEventTimestamp {
    pub fn from_rt(
        sample_count: u64,
        sample_rate: Hz,
        intra_block_offset: DurationInSeconds,
    ) -> Self {
        let start_secs = sample_count as f64 / sample_rate.get();
        let final_secs = start_secs + intra_block_offset.get();
        Self(Duration::from_secs_f64(final_secs))
    }
}

impl Sub for ControlEventTimestamp {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0.saturating_sub(rhs.0)
    }
}

impl Display for ControlEventTimestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
