#![allow(dead_code)]

use crate::conversion_util::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
};
use crate::rt::supplier::midi_util::supply_midi_material;
use crate::rt::supplier::time_series::{TimeSeries, TimeSeriesEvent};
use crate::rt::supplier::{
    MaterialInfo, MidiMaterialInfo, MidiSupplier, SupplyMidiRequest, SupplyResponse,
    WithMaterialInfo, MIDI_FRAME_RATE,
};
use crate::ClipEngineResult;
use helgoboss_midi::{RawShortMessage, ShortMessage, ShortMessageFactory};
use playtime_api::persistence::{Bpm, TimeSignature};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, Hz, MidiFrameOffset};
use std::cmp;
use std::error::Error;
use std::fmt::{Display, Formatter, Write};

#[derive(Clone, Debug, Default)]
pub struct MidiSequence {
    ppq: u64,
    time_series: TimeSeries<MidiEventPayload>,
    time_info: MidiTimeInfo,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MidiTimeInfo {
    tempo: Bpm,
    time_signature: TimeSignature,
}

impl Default for MidiTimeInfo {
    fn default() -> Self {
        Self {
            tempo: Bpm::new(960.0).unwrap(),
            time_signature: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
        }
    }
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
    HasData(HasDataEntry),
    Event(MidiEvent),
    IgnTempo(MidiTimeInfo),
    Unhandled(&'a str),
}

struct HasDataEntry {
    ppq: u64,
}

impl MidiSequence {
    pub fn parse_from_reaper_midi_chunk(chunk: &str) -> Result<Self, Box<dyn Error>> {
        let mut sequence = Self::default();
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
                ReaperMidiChunkEntry::HasData(e) => {
                    sequence.ppq = e.ppq;
                }
                ReaperMidiChunkEntry::IgnTempo(d) => {
                    sequence.time_info = d;
                }
                ReaperMidiChunkEntry::Unhandled(_) => {}
            }
        }
        sequence.time_series = TimeSeries::new(events);
        Ok(sequence)
    }

    pub fn format_as_reaper_midi_chunk(&self) -> String {
        MidiSequenceAsReaperChunk(self).to_string()
    }

    pub fn total_pulse_count(&self) -> u64 {
        // The last pulse frame index marks the *exclusive* end, so we don't need + 1 here I guess.
        self.time_series.events.last().map(|e| e.frame).unwrap_or(0)
    }

    pub fn predefined_length(&self) -> DurationInSeconds {
        self.length_at(
            self.time_info.tempo,
            self.time_info.time_signature.denominator,
        )
    }

    fn length_at(&self, bpm: Bpm, time_sig_denominator: u32) -> DurationInSeconds {
        DurationInSeconds::new(self.convert_pulse_to_second_flexible(
            self.total_pulse_count(),
            bpm,
            time_sig_denominator,
        ))
    }

    fn midi_frame_count(&self) -> usize {
        let length = self.predefined_length();
        convert_duration_in_seconds_to_frames(length, MIDI_FRAME_RATE)
    }

    fn convert_pulse_to_duration_in_seconds(&self, pulse: u64) -> DurationInSeconds {
        let seconds = self.convert_pulse_to_second_flexible(
            pulse,
            self.time_info.tempo,
            self.time_info.time_signature.denominator,
        );
        DurationInSeconds::new(seconds)
    }

    fn calculate_pulse_to_midi_frame_factor(&self, frame_rate: Hz) -> f64 {
        let pulse_to_second_factor = self.convert_pulse_to_second_flexible(
            1,
            self.time_info.tempo,
            self.time_info.time_signature.denominator,
        );
        pulse_to_second_factor * frame_rate.get()
    }

    fn convert_pulse_to_second_flexible(
        &self,
        pulse: u64,
        bpm: Bpm,
        time_sig_denominator: u32,
    ) -> f64 {
        let bps = bpm.get() / 60.0;
        let duration_of_one_beat_in_secs = 1.0 / bps;
        let quarter_notes = pulse as f64 / self.ppq as f64;
        let beat = quarter_notes * get_qn_to_beat_factor(time_sig_denominator);
        beat * duration_of_one_beat_in_secs
    }

    fn convert_duration_in_seconds_to_pulse(&self, duration: DurationInSeconds) -> u64 {
        self.convert_second_to_pulse_flexible(
            duration.get(),
            self.time_info.tempo,
            self.time_info.time_signature.denominator,
        ) as u64
    }

    fn convert_second_to_pulse_flexible(
        &self,
        second: f64,
        bpm: Bpm,
        time_sig_denominator: u32,
    ) -> i64 {
        let bps = bpm.get() / 60.0;
        let duration_of_one_beat_in_secs = 1.0 / bps;
        let beat = second / duration_of_one_beat_in_secs;
        let quarter_notes = beat / get_qn_to_beat_factor(time_sig_denominator);
        let pulse = quarter_notes * self.ppq as f64;
        pulse.floor() as i64
    }
}

/// Calculates the factor for converting from quarter notes to beats.
///
/// 1 quarter note = how many beats?
/// - x/4 => 1 beat
/// - x/8 => 2 beats
/// - x/2 => 0.5 beats
fn get_qn_to_beat_factor(time_sig_denominator: u32) -> f64 {
    time_sig_denominator as f64 / 4.0
}

impl MidiSupplier for MidiSequence {
    // Below logic assumes that the destination frame rate is comparable to the source frame
    // rate. The resampler makes sure of it. However, it's not necessarily equal since we use
    // frame rate changes for tempo changes. It's only equal if the clip is played in
    // MIDI_BASE_BPM. That's fine!
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        supply_midi_material(request, |request| {
            // If sequence empty, stop right here.
            let total_pulse_count = self.total_pulse_count();
            if total_pulse_count == 0 {
                return SupplyResponse::exceeded_end();
            }
            let frame_rate = request.dest_sample_rate;
            let num_frames_to_be_consumed = request.dest_frame_count;
            let start_time = convert_duration_in_frames_to_seconds(request.start_frame, frame_rate);
            let start_pulse = self.convert_duration_in_seconds_to_pulse(start_time);
            let requested_length =
                convert_duration_in_frames_to_seconds(num_frames_to_be_consumed, frame_rate);
            if start_pulse >= total_pulse_count {
                return SupplyResponse::exceeded_end();
            };
            let requested_pulse_count = self.convert_duration_in_seconds_to_pulse(requested_length);
            let remaining_pulse_count_till_end = total_pulse_count - start_pulse;
            let actual_requested_pulse_count =
                cmp::min(requested_pulse_count, remaining_pulse_count_till_end);
            let pulse_to_frame_factor = self.calculate_pulse_to_midi_frame_factor(frame_rate);
            let mut reaper_evt = reaper_medium::MidiEvent::default();
            let relevant_events = self
                .time_series
                .find_events_in_range(start_pulse, actual_requested_pulse_count);
            for evt in relevant_events {
                let pulse_offset = (evt.frame - start_pulse) as f64;
                let frame_offset = (pulse_offset * pulse_to_frame_factor).floor() as u32;
                reaper_evt.set_frame_offset(MidiFrameOffset::new(frame_offset));
                reaper_evt.set_message(evt.payload.msg);
                event_list.add_item(&reaper_evt)
            }
            let num_midi_frames_consumed =
                (actual_requested_pulse_count as f64 * pulse_to_frame_factor).floor() as usize;
            // TODO We don't include the last frame when it comes to reporting the pulse/frame count.
            //  Should we manually add this notes-off event? Or is it already transmitted? Or should we
            //  handle this via StartEndHandler?
            // The lower the sample rate, the higher the tempo, the more inner source material we
            // effectively grabbed.
            SupplyResponse::limited_by_total_frame_count(
                num_midi_frames_consumed,
                num_midi_frames_consumed,
                request.start_frame as isize,
                self.midi_frame_count(),
            )
        })
    }
}

impl WithMaterialInfo for MidiSequence {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        let info = MidiMaterialInfo {
            frame_count: self.midi_frame_count(),
        };
        Ok(MaterialInfo::Midi(info))
    }
}

struct MidiSequenceAsReaperChunk<'a>(&'a MidiSequence);

impl<'a> Display for MidiSequenceAsReaperChunk<'a> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "HASDATA 1 {} QN", self.0.ppq)?;
        let mut last_frame = 0;
        for e in &self.0.time_series.events {
            e.format_for_reaper_chunk(f, &mut last_frame)?;
            f.write_char('\n')?;
        }
        let info = &self.0.time_info;
        writeln!(
            f,
            "IGNTEMPO 1 {} {} {}",
            info.tempo.get(),
            info.time_signature.numerator,
            info.time_signature.denominator
        )?;
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
        // Example: "em 240 81 37 00 -90"
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
        // Example: "HASDATA 1 960 QN"
        "HASDATA" => {
            iter.next().ok_or("no HASDATA flag")?;
            let ppq: u64 = iter.next().ok_or("no PPQ")?.parse()?;
            let unit = iter.next().ok_or("no PPQ unit")?;
            if unit != "QN" {
                return Err("no QN unit".into());
            }
            let entry = HasDataEntry { ppq };
            Ok(ReaperMidiChunkEntry::HasData(entry))
        }
        // Example: "IGNTEMPO 1 120 4 4"
        "IGNTEMPO" => {
            iter.next().ok_or("no IGNTEMPO flag")?;
            let tempo: f64 = iter.next().ok_or("no custom tempo")?.parse()?;
            let numerator: u32 = iter.next().ok_or("no custom numerator")?.parse()?;
            let denominator: u32 = iter.next().ok_or("no custom denominator")?.parse()?;
            let data = MidiTimeInfo {
                tempo: Bpm::new(tempo)?,
                time_signature: TimeSignature {
                    numerator,
                    denominator,
                },
            };
            Ok(ReaperMidiChunkEntry::IgnTempo(data))
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
    fn basics() {
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
            ppq: 960,
            time_series: TimeSeries::new(events),
            time_info: MidiTimeInfo {
                tempo: Bpm::new(120.0).unwrap(),
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            },
        };
        // When
        assert_eq!(midi_sequence.total_pulse_count(), 960);
        let predefined_length = midi_sequence.predefined_length();
        assert_eq!(predefined_length.get(), 0.5);
        let length_with_normal_tempo = midi_sequence.length_at(Bpm::new(120.0).unwrap(), 4);
        assert_eq!(length_with_normal_tempo.get(), 0.5);
        let length_with_different_time_sig = midi_sequence.length_at(Bpm::new(120.0).unwrap(), 8);
        assert_eq!(length_with_different_time_sig.get(), 1.0);
        let length_with_double_tempo = midi_sequence.length_at(Bpm::new(240.0).unwrap(), 4);
        assert_eq!(length_with_double_tempo.get(), 0.25);
        let length_with_half_tempo = midi_sequence.length_at(Bpm::new(60.0).unwrap(), 4);
        assert_eq!(length_with_half_tempo.get(), 1.0);
    }

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
        assert_eq!(sequence.ppq, 960);
        assert_eq!(
            sequence.time_info,
            MidiTimeInfo {
                tempo: Bpm::new(120.0).unwrap(),
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            }
        );
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
            ppq: 960,
            time_series: TimeSeries::new(events),
            time_info: MidiTimeInfo {
                tempo: Bpm::new(120.0).unwrap(),
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            },
        };
        // When
        let actual_chunk = midi_sequence.format_as_reaper_midi_chunk();
        // Then
        let expected_chunk = r#"HASDATA 1 960 QN
e 0 91 30 31 0
E 1 b0 7b 00 0
e 239 81 30 00 -90
e 240 91 37 27 0
em 240 81 37 00 -90
E 240 b0 7b 00 0
IGNTEMPO 1 120 4 4
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
