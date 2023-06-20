#![allow(dead_code)]
use crate::rt::supplier::time_series::TimeSeries;
use helgoboss_midi::RawShortMessage;

#[derive(Clone, Debug)]
pub struct MidiSequence {
    time_series: TimeSeries<MidiEvent>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MidiEvent {
    pub selected: bool,
    pub mute: bool,
    pub msg: RawShortMessage,
    /// Positive means it has been shifted to the right, negative to the left.
    pub quantization_shift: i32,
}

impl MidiSequence {
    pub fn parse_from_reaper_midi_chunk(_chunk: &str) -> Result<Self, &'static str> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::supplier::time_series::TimeSeriesEntry;
    use helgoboss_midi::test_util::*;

    #[test]
    fn parse_from_reaper_midi_chunk() {
        // Given
        let chunk = r#"
e 0 91 30 31
E 1 b0 7b 00
e 239 81 30 00 -90
e 240 91 37 27
em 240 81 37 00 -90
E 240 b0 7b 00
"#;
        // When
        let sequence = MidiSequence::parse_from_reaper_midi_chunk(chunk).unwrap();
        // Then
        assert_eq!(
            sequence.time_series.entries,
            vec![
                // e 0 91 30 31
                e(0, true, false, (0x91, 0x30, 0x31), 0),
                // E 1 b0 7b 00
                e(1, false, false, (0xb0, 0x7b, 0x00), 0),
                // e 239 81 30 00 -90
                e(240, true, false, (0x81, 0x30, 0x00), 90),
                // e 240 91 37 27
                e(480, true, false, (0x91, 0x37, 0x27), 0),
                // em 240 81 37 00 -90
                e(720, true, true, (0x81, 0x37, 0x00), 90),
                // E 240 b0 7b 00
                e(960, false, false, (0xb0, 0x7b, 0x00), 0),
            ]
        );
    }

    fn e(
        frame: u64,
        selected: bool,
        mute: bool,
        (byte1, byte2, byte3): (u8, u8, u8),
        quantization_shift: i32,
    ) -> TimeSeriesEntry<MidiEvent> {
        let evt = MidiEvent {
            selected,
            mute,
            msg: short(byte1, byte2, byte3),
            quantization_shift,
        };
        TimeSeriesEntry::new(frame, evt)
    }
}
