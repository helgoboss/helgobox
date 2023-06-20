#![allow(dead_code)]

use crate::rt::supplier::time_series::{TimeSeries, TimeSeriesEvent};
use helgoboss_midi::{RawShortMessage, ShortMessage, ShortMessageFactory};
use std::error::Error;
use std::fmt::{Display, Formatter, Write};

#[derive(Clone, Debug)]
pub struct MidiSequence {
    time_series: TimeSeries<MidiEventPayload>,
}

pub type MidiEvent = TimeSeriesEvent<MidiEventPayload>;

impl MidiEvent {
    pub fn format_for_reaper_chunk(
        &self,
        f: &mut Formatter,
        last_frame: &mut u64,
    ) -> std::fmt::Result {
        let selected_char = if self.payload.selected { 'e' } else { 'E' };
        let mute_str = if self.payload.mute { "m" } else { "" };
        let frame_diff = self.frame - *last_frame;
        *last_frame = self.frame;
        let msg = self.payload.msg;
        let b1 = msg.status_byte();
        let b2 = msg.data_byte_1().get();
        let b3 = msg.data_byte_2().get();
        let negative_quantization_shift = -self.payload.quantization_shift;
        write!(
            f,
            "{selected_char}{mute_str} {frame_diff} {b1:02x} {b2:02x} {b3:02x} {negative_quantization_shift}"
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MidiEventPayload {
    pub selected: bool,
    pub mute: bool,
    pub msg: RawShortMessage,
    /// Positive means it has been shifted to the right, negative to the left.
    pub quantization_shift: i32,
}

enum ReaperMidiChunkEntry<'a> {
    Event(MidiEvent),
    Unhandled(&'a str),
}

impl MidiSequence {
    pub fn parse_from_reaper_midi_chunk(chunk: &str) -> Result<Self, Box<dyn Error>> {
        let mut last_frame = 0;
        let mut events = Vec::new();
        for line in chunk.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match parse_entry(trimmed, &mut last_frame)? {
                ReaperMidiChunkEntry::Event(e) => {
                    events.push(e);
                }
                ReaperMidiChunkEntry::Unhandled(_) => {}
            }
        }
        let sequence = Self {
            time_series: TimeSeries::new(events),
        };
        Ok(sequence)
    }

    pub fn format_as_reaper_midi_chunk(&self) -> String {
        MidiSequenceAsReaperChunk(self).to_string()
    }
}

struct MidiSequenceAsReaperChunk<'a>(&'a MidiSequence);

impl<'a> Display for MidiSequenceAsReaperChunk<'a> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut last_frame = 0;
        for e in &self.0.time_series.events {
            e.format_for_reaper_chunk(f, &mut last_frame)?;
            f.write_char('\n')?;
        }
        Ok(())
    }
}

fn parse_entry<'a>(
    line: &'a str,
    last_frame: &mut u64,
) -> Result<ReaperMidiChunkEntry<'a>, Box<dyn Error>> {
    let mut iter = line.split(' ');
    let directive = iter.next().ok_or("missing directive")?;
    match directive {
        "e" | "E" | "em" | "Em" => {
            let mut prefix_chars = directive.chars();
            let selected = prefix_chars.next().ok_or("missing selected char")? == 'e';
            let mute = prefix_chars.next() == Some('m');
            let frame_diff: u64 = iter.next().ok_or("missing frame diff")?.parse()?;
            let frame = *last_frame + frame_diff;
            *last_frame = frame;
            let msg = RawShortMessage::from_bytes((
                parse_hex_byte(&mut iter)?,
                parse_hex_byte(&mut iter)?,
                parse_hex_byte(&mut iter)?,
            ))?;
            let quantization_shift = match iter.next() {
                None => 0,
                Some(s) => {
                    let negative_shift: i32 = s.parse()?;
                    -negative_shift
                }
            };
            let event = MidiEvent::new(
                frame,
                MidiEventPayload {
                    selected,
                    mute,
                    msg,
                    quantization_shift,
                },
            );
            Ok(ReaperMidiChunkEntry::Event(event))
        }
        _ => Ok(ReaperMidiChunkEntry::Unhandled(line)),
    }
}

fn parse_hex_byte<'a, T: TryFrom<u8, Error = E>, E: Error + 'static>(
    iter: &mut impl Iterator<Item = &'a str>,
) -> Result<T, Box<dyn Error>> {
    let hex_string = iter.next().ok_or("byte missing")?;
    let byte = u8::from_str_radix(hex_string, 16)?;
    Ok(byte.try_into()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::supplier::time_series::TimeSeriesEvent;
    use helgoboss_midi::test_util::*;

    #[test]
    fn parse_from_reaper_midi_chunk() {
        // Given
        let chunk = r#"
HASDATA 1 960 QN
CCINTERP 32
POOLEDEVTS {2B7731B1-2DE0-534E-A08F-DBFB0B3205DC}
e 0 91 30 31
E 1 b0 7b 00
e 239 81 30 00 -90
e 240 91 37 27
em 240 81 37 00 -90
E 240 b0 7b 00
CCINTERP 32
GUID {ACC4D7CA-2E56-0248-AD95-8B027F12FD09}
IGNTEMPO 1 120 4 4
SRCCOLOR 8197
"#;
        // When
        let sequence = MidiSequence::parse_from_reaper_midi_chunk(chunk).unwrap();
        // Then
        assert_eq!(
            sequence.time_series.events,
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

    #[test]
    fn format_as_reaper_midi_chunk() {
        // Given
        let events = vec![
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
        ];
        let midi_sequence = MidiSequence {
            time_series: TimeSeries::new(events),
        };
        // When
        let actual_chunk = midi_sequence.format_as_reaper_midi_chunk();
        // Then

        let expected_chunk = r#"e 0 91 30 31 0
E 1 b0 7b 00 0
e 239 81 30 00 -90
e 240 91 37 27 0
em 240 81 37 00 -90
E 240 b0 7b 00 0
"#;
        // Then
        assert_eq!(actual_chunk.as_str(), expected_chunk);
    }

    fn e(
        frame: u64,
        selected: bool,
        mute: bool,
        (byte1, byte2, byte3): (u8, u8, u8),
        quantization_shift: i32,
    ) -> TimeSeriesEvent<MidiEventPayload> {
        let evt = MidiEventPayload {
            selected,
            mute,
            msg: short(byte1, byte2, byte3),
            quantization_shift,
        };
        TimeSeriesEvent::new(frame, evt)
    }
}
