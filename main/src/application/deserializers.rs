use serde::{Deserialize, Deserializer};
use std::convert::TryFrom;

/// Behaves like the built-in deserializer for `Option` but also accepts `-1` as `None`.
///
/// Also accepts decimal numbers.
///
/// Based on this: https://stackoverflow.com/a/56384732/5418870
pub fn none_if_minus_one<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + TryFrom<u64>,
{
    // we define a local enum type inside of the function
    // because it is untagged, serde will deserialize as the first variant
    // that it can
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeMinusOne<U> {
        // If it can be parsed as Option<T>, it's perfect.
        TargetValue(Option<U>),
        // Otherwise try parsing as decimal number.
        DecimalNumber(f64),
    }

    let value: MaybeMinusOne<T> = Deserialize::deserialize(deserializer)?;
    match value {
        MaybeMinusOne::TargetValue(v) => Ok(v),
        MaybeMinusOne::DecimalNumber(n) => {
            if n == -1.0 {
                Ok(None)
            } else {
                match T::try_from(n as u64) {
                    Ok(t) => Ok(Some(t)),
                    Err(_) => Err(serde::de::Error::custom("invalid number")),
                }
            }
        }
    }
}

/// This is like the u32 deserializer but is also fine with decimal numbers (e.g. 4.0).
///
/// For some reason we have serialized positive integers as floating point numbers in ReaLearn C++.
pub fn f32_as_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value: f32 = Deserialize::deserialize(deserializer)?;
    let integer = value as i32;
    if integer < 0 {
        return Err(serde::de::Error::custom("number must not be negative"));
    }
    Ok(integer as u32)
}
