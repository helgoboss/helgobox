use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

/// An approximate floating-point type that uses the same epsilon for comparison as the == operator in
/// [EEL2](https://www.cockos.com/EEL2/):
/// Two values are considered equal if the difference is less than 0.00001 (1/100000), 0 if not.
pub type AudioF64 = ApproxF64<100000>;

/// Simple newtype that allows for approximate comparison of 64-bit floating-point numbers.
///
/// The const type parameter `E` ("epsilon") defines how tolerant floating-point comparison is. Two values are considered
/// equal if the difference is less than 1/E.
#[derive(Copy, Clone, Debug, Default)]
pub struct ApproxF64<const E: u32>(pub f64);

impl<const E: u32> ApproxF64<E> {
    const EPSILON: f64 = 1.0 / E as f64;

    pub fn new(raw: f64) -> Self {
        Self(raw)
    }

    fn difference_is_neglectable(&self, other: &Self) -> bool {
        (self.0 - other.0).abs() < Self::EPSILON
    }
}

impl<const E: u32> PartialOrd for ApproxF64<E> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.difference_is_neglectable(other) {
            return Some(Ordering::Equal);
        }
        self.0.partial_cmp(&other.0)
    }
}

impl<const E: u32> PartialEq for ApproxF64<E> {
    fn eq(&self, other: &Self) -> bool {
        self.difference_is_neglectable(other)
    }
}

impl<const E: u32> Display for ApproxF64<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(AudioF64::new(0.75), AudioF64::new(0.75));
        assert_ne!(AudioF64::new(0.00001), AudioF64::new(0.00002));
        assert_eq!(AudioF64::new(0.000001), AudioF64::new(0.000002));
    }
}
