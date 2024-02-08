use serde::{Deserialize, Deserializer};

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
