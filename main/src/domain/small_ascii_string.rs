use ascii::{AsciiStr, AsciiString};
use core::fmt;

/// String with a maximum of 16 ASCII characters.
///
/// It's useful in the audio thread because it can be cheaply copied and doesn't need allocation.
/// If you are okay with allocation and need cheap cloning, you could just as well use an
/// `Rc<String>`.
pub type SmallAsciiString = LimitedAsciiString<16>;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct LimitedAsciiString<const N: usize> {
    length: u8,
    content: [u8; SmallAsciiString::MAX_LENGTH],
}

impl<const N: usize> LimitedAsciiString<N> {
    pub const MAX_LENGTH: usize = N;

    /// Crops the string if necessary.
    pub fn from_ascii_str_cropping(ascii_str: &AsciiStr) -> Self {
        let short =
            AsciiString::from(&ascii_str.as_slice()[..Self::MAX_LENGTH.min(ascii_str.len())]);
        Self::from_ascii_str(&short)
    }

    /// Returns an error if the given string is too long.
    pub fn try_from_ascii_str(ascii_str: &AsciiStr) -> Result<Self, &'static str> {
        if ascii_str.len() > SmallAsciiString::MAX_LENGTH {
            return Err("ASCII string too large");
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

impl<const N: usize> fmt::Display for LimitedAsciiString<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ascii_str().fmt(f)
    }
}
