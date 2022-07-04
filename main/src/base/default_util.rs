use helgoboss_learn::UnitValue;
use serde::{Deserialize, Deserializer};

pub fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}

pub fn bool_true() -> bool {
    true
}

pub fn is_bool_true(v: &bool) -> bool {
    *v
}

pub fn unit_value_one() -> UnitValue {
    UnitValue::MAX
}

pub fn is_unit_value_one(v: &UnitValue) -> bool {
    *v == UnitValue::MAX
}

/// Should only be used when the deserialization checks the data version number because only that
/// way it can check if `None` represents the old default or the new one! (That is, if there's
/// even a difference between `None` and `Some(default())`, otherwise it doesn't matter).
pub fn is_none_or_some_default<T: Default + PartialEq>(v: &Option<T>) -> bool {
    if let Some(i) = v {
        i == &T::default()
    } else {
        true
    }
}

/// Makes sure that JSON `null` is treated the same as omitting a property.
///
/// Use as `#[serde(deserialize_with = "deserialize_null_default")]`.
///
/// See https://github.com/serde-rs/serde/issues/1098#issuecomment-760711617.
pub fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}
