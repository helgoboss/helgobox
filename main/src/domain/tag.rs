use crate::domain::SmallAsciiString;
use ascii::{AsciiChar, AsciiString, ToAsciiChar};
use core::fmt;
use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

/// We reduce the number of possible letters in case we want to use tags in the audio thread in
/// future (and therefore need to avoid allocation).
#[derive(
    Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, SerializeDisplay, DeserializeFromStr,
)]
pub struct Tag(SmallAsciiString);

impl FromStr for Tag {
    type Err = &'static str;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let ascii_string: AsciiString = text
            .chars()
            // Remove all non-ASCII schars
            .filter_map(|c| c.to_ascii_char().ok())
            // Allow only letters, digits and underscore
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == AsciiChar::UnderScore)
            // Skip leading digits
            .skip_while(|ch| ch.is_ascii_digit())
            // No uppercase
            .map(|ch| ch.to_ascii_lowercase())
            .collect();
        if ascii_string.is_empty() {
            return Err("empty tag");
        }
        let small_ascii_string = SmallAsciiString::from_ascii_str_cropping(&ascii_string);
        Ok(Self(small_ascii_string))
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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
