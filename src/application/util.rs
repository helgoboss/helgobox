use serde::{Deserialize, Deserializer};

/// Behaves like the built-in deserializer for `Option` but also accepts `-1` as `None`.
///
/// Based on this: https://stackoverflow.com/a/56384732/5418870
pub fn none_if_minus_one<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // we define a local enum type inside of the function
    // because it is untagged, serde will deserialize as the first variant
    // that it can
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeMinusOne<U> {
        // if it can be parsed as Option<T>, it will be
        TargetValue(Option<U>),
        // otherwise try parsing as integer
        IncompatibleInteger(i8),
    }

    let value: MaybeMinusOne<T> = Deserialize::deserialize(deserializer)?;
    match value {
        MaybeMinusOne::TargetValue(v) => Ok(v),
        MaybeMinusOne::IncompatibleInteger(i) => {
            if i == -1 {
                Ok(None)
            } else {
                Err(serde::de::Error::custom("invalid number"))
            }
        }
    }
}
