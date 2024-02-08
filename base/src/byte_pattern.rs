//! We have a raw MIDI pattern in helgoboss-learn already (raw MIDI source), however this is more
//! complicated than this one as it also allows single bits to be variable.

use logos::{Lexer, Logos};
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct BytePattern {
    bytes: Vec<PatternByte>,
}

impl BytePattern {
    pub const fn new(bytes: Vec<PatternByte>) -> Self {
        Self { bytes }
    }

    pub fn matches(&self, bytes: &[u8]) -> bool {
        use PatternByte::*;
        let mut byte_iter = bytes.iter();
        let mut last_was_multi = false;
        for pattern_byte in &self.bytes {
            let matches = match pattern_byte {
                Fixed(expected_byte) => {
                    if last_was_multi {
                        // Last pattern byte was multi
                        last_was_multi = false;
                        // Greedily consume any follow-up actual bytes until we meet the expected
                        // byte. If we don't meet it, no match!
                        byte_iter.any(|b| b == expected_byte)
                    } else {
                        // Last pattern byte was single or fixed
                        byte_iter.next().is_some_and(|b| b == expected_byte)
                    }
                }
                Single => {
                    last_was_multi = false;
                    // We need to have an actual byte but it doesn't matter which one!
                    byte_iter.next().is_some()
                }
                Multi => {
                    last_was_multi = true;
                    // Match even if no actual byte left!
                    true
                }
            };
            if !matches {
                return false;
            }
        }
        byte_iter.next().is_none() || last_was_multi
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Logos)]
#[logos(skip r"[ \t\n\f]+")]
#[logos(error = ParseBytePatternError)]
pub enum PatternByte {
    #[regex(r"[0-9a-fA-F][0-9a-fA-F]?", parse_as_byte)]
    Fixed(u8),
    #[token("?")]
    Single,
    #[token("*")]
    Multi,
}

#[derive(Clone, PartialEq, Debug, Default, thiserror::Error)]
#[error("{msg}")]
pub struct ParseBytePatternError {
    msg: &'static str,
}

impl From<ParseIntError> for ParseBytePatternError {
    fn from(_: ParseIntError) -> Self {
        Self {
            msg: "problem parsing fixed byte",
        }
    }
}

fn parse_as_byte(lex: &mut Lexer<PatternByte>) -> Result<u8, ParseIntError> {
    u8::from_str_radix(lex.slice(), 16)
}

impl FromStr for BytePattern {
    type Err = ParseBytePatternError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lex: Lexer<PatternByte> = PatternByte::lexer(s);
        let entries: Result<Vec<_>, _> = lex.collect();
        Ok(BytePattern::new(entries?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        // Given
        let pattern: BytePattern = "F0 7E ? 06 02 * F7".parse().unwrap();
        // When
        assert!(!pattern.matches(&[]));
        assert!(!pattern.matches(&[0xF0]));
        assert!(!pattern.matches(&[0xF0, 0x7E]));
        assert!(!pattern.matches(&[0xF0, 0x7E, 0x00]));
        assert!(!pattern.matches(&[0xF0, 0x7E, 0x00, 0x06]));
        assert!(pattern.matches(&[0xF0, 0x7E, 0x00, 0x06, 0x02, 0xF7]));
        assert!(pattern.matches(&[0xF0, 0x7E, 0x01, 0x06, 0x02, 0xF7]));
        assert!(pattern.matches(&[0xF0, 0x7E, 0xFF, 0x06, 0x02, 0xFF, 0x60, 0xF7]));
        assert!(!pattern.matches(&[0xF0, 0x7E, 0xFF, 0x06, 0x02, 0xFF, 0x60, 0xF7, 0xF7]));
    }
}
