use helgoboss_learn::AbstractTimestamp;
use std::fmt::{Display, Formatter};
use std::ops::Sub;
use std::time::{Duration, Instant};

pub type ControlEvent<P> = helgoboss_learn::ControlEvent<P, ControlEventTimestamp>;

/// Timestamp of a control event.
//
// Don't expose the inner field, it should stay private. We might swap the time unit in future to
// improve performance and accuracy.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ControlEventTimestamp(Instant);

impl AbstractTimestamp for ControlEventTimestamp {
    fn now() -> Self {
        Self(Instant::now())
    }
}

impl Sub for ControlEventTimestamp {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.0 - rhs.0
    }
}

impl Display for ControlEventTimestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
