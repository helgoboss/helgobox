use ascii::{AsciiChar, AsciiStr, AsciiString, ToAsciiChar};
use core::fmt;

/// String with a maximum of 32 ASCII characters.
///
/// It's useful in the audio thread because it can be cheaply copied and doesn't need allocation.
/// If you are okay with allocation and need cheap cloning, you could just as well use an
/// `Rc<String>`.
pub type SmallAsciiString = LimitedAsciiString<32>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct LimitedAsciiString<const N: usize> {
    length: u8,
    content: [u8; N],
}

impl<const N: usize> LimitedAsciiString<N> {
    pub const MAX_LENGTH: usize = N;

    /// Crops the string if necessary.
    pub fn from_ascii_str_cropping(ascii_str: &AsciiStr) -> Self {
        let short =
            AsciiString::from(&ascii_str.as_slice()[..Self::MAX_LENGTH.min(ascii_str.len())]);
        Self::from_ascii_str(&short)
    }

    /// Returns an error if the given string is not completely ASCII or is too long.
    pub fn try_from_str(value: &str) -> Result<Self, &'static str> {
        let ascii_string: Result<AsciiString, _> =
            value.chars().map(|c| c.to_ascii_char()).collect();
        let ascii_string = ascii_string.map_err(|_| "value contains non-ASCII characters")?;
        Self::try_from_ascii_str(&ascii_string)
    }

    /// Returns an error if the given string is too long.
    pub fn try_from_ascii_str(ascii_str: &AsciiStr) -> Result<Self, &'static str> {
        if ascii_str.len() > Self::MAX_LENGTH {
            return Err("ASCII string too large");
        }
        Ok(Self::from_ascii_str(ascii_str))
    }

    /// Panics if the given string is too long.
    fn from_ascii_str(ascii_str: &AsciiStr) -> Self {
        let mut content = [0u8; N];
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

pub fn convert_to_identifier(text: &str) -> Result<SmallAsciiString, &'static str> {
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
    Ok(SmallAsciiString::from_ascii_str_cropping(&ascii_string))
}

impl<const N: usize> fmt::Display for LimitedAsciiString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ascii_str().fmt(f)
    }
}
