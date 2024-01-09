//! We have a raw MIDI pattern in helgoboss-learn already (raw MIDI source), however this is more
//! complicated than this one as it also allows single bits to be variable.

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct BytePattern<const N: usize> {
    bytes: [PatternByte; N],
}

impl<const N: usize> BytePattern<N> {
    pub const fn new(bytes: [PatternByte; N]) -> Self {
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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PatternByte {
    Fixed(u8),
    Single,
    Multi,
}

#[cfg(test)]
mod tests {
    use super::*;
    use Fixed as F;
    use PatternByte::*;

    #[test]
    fn basics() {
        // Given
        let pattern =
            BytePattern::new([F(0xF0), F(0x7E), Single, F(0x06), F(0x02), Multi, F(0xF7)]);
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
