use std::fmt::{Display, Formatter};

pub fn parse_hex_string(value: &str) -> Result<Vec<u8>, hex::FromHexError> {
    let without_spaces = value.replace(' ', "");
    hex::decode(without_spaces)
}

/// Formats the given slice of bytes as hex numbers separated by spaces.
pub fn format_as_pretty_hex(bytes: &[u8]) -> String {
    DisplayAsPrettyHex(bytes).to_string()
}

pub struct DisplayAsPrettyHex<'a>(pub &'a [u8]);

impl Display for DisplayAsPrettyHex<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{:02X?}", *b)?;
        }
        Ok(())
    }
}
