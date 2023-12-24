use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct InstanceId(u32);

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl InstanceId {
    pub fn next() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl From<u32> for InstanceId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<InstanceId> for u32 {
    fn from(value: InstanceId) -> Self {
        value.0
    }
}
