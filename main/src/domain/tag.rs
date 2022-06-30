use crate::domain::{convert_to_identifier, SmallAsciiString};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::str::FromStr;

/// We reduce the number of possible letters in case we want to use tags in the audio thread in
/// future (and therefore need to avoid allocation).
#[derive(
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Debug,
    Hash,
    derive_more::Display,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub struct Tag(SmallAsciiString);

impl FromStr for Tag {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let small_ascii_string = convert_to_identifier(text)?;
        Ok(Self(small_ascii_string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn parse_tags() {
        assert_eq!(Tag::from_str("hey").unwrap().to_string(), "hey");
        assert_eq!(Tag::from_str("_hey").unwrap().to_string(), "_hey");
        assert_eq!(Tag::from_str("HeY").unwrap().to_string(), "hey");
        assert_eq!(Tag::from_str("hey_test").unwrap().to_string(), "hey_test");
        assert_eq!(
            Tag::from_str("1ähey1ätest").unwrap().to_string(),
            "hey1test"
        );
        assert!(Tag::from_str("1ä").is_err());
    }
}
