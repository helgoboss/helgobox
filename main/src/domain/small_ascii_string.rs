use ascii::{AsciiStr, AsciiString};
use core::fmt;

/// String with a maximum of 16 ASCII characters.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct SmallAsciiString {
    length: u8,
    content: [u8; SmallAsciiString::MAX_LENGTH],
}

impl SmallAsciiString {
    pub const MAX_LENGTH: usize = 16;

    /// Crops the string if necessary.
    pub fn from_ascii_str_cropping(ascii_str: &AsciiStr) -> Self {
        let short =
            AsciiString::from(&ascii_str.as_slice()[..Self::MAX_LENGTH.min(ascii_str.len())]);
        Self::from_ascii_str(&short)
    }

    /// Returns an error if the given string is too long.
    pub fn try_from_ascii_str(ascii_str: &AsciiStr) -> Result<Self, &'static str> {
        if ascii_str.len() > SmallAsciiString::MAX_LENGTH {
            return Err("too large to be a small ASCII string");
        }
        Ok(Self::from_ascii_str(ascii_str))
    }

    /// Panics if the given string is too long.
    fn from_ascii_str(ascii_str: &AsciiStr) -> Self {
        let mut content = [0u8; SmallAsciiString::MAX_LENGTH];
        content[..ascii_str.len()].copy_from_slice(ascii_str.as_bytes());
        Self {
            content,
            length: ascii_str.len() as u8,
        }
    }

    pub fn as_ascii_str(&self) -> &AsciiStr {
        AsciiStr::from_ascii(self.as_slice()).unwrap()
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.content[..(self.length as usize)]
    }
}

impl fmt::Display for SmallAsciiString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ascii_str().fmt(f)
    }
}
