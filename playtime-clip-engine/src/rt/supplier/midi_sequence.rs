#![allow(dead_code)]

use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_seconds,
};
use crate::rt::supplier::midi_util::supply_midi_material;
use crate::rt::supplier::time_series::{TimeSeries, TimeSeriesEvent};
use crate::rt::supplier::{
    MaterialInfo, MidiMaterialInfo, MidiSupplier, SupplyMidiRequest, SupplyResponse,
    WithMaterialInfo, MIDI_BASE_BPM, MIDI_FRAME_RATE,
};
use crate::ClipEngineResult;
use helgoboss_midi::{RawShortMessage, ShortMessage, ShortMessageFactory};
use playtime_api::persistence::{Bpm, TimeSignature};
use reaper_medium::{BorrowedMidiEventList, DurationInSeconds, MidiFrameOffset};
use std::cmp;
use std::error::Error;
use std::fmt::{Display, Formatter, Write};

#[derive(Clone, Debug)]
pub struct MidiSequence {
    ppq: u64,
    time_series: TimeSeries<MidiEventPayload>,
    time_info: MidiTimeInfo,
    normalized_second_to_pulse_factor: f64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MidiTimeInfo {
    igntempo: bool,
    tempo: Bpm,
    time_signature: TimeSignature,
}

impl Default for MidiTimeInfo {
    fn default() -> Self {
        Self {
            igntempo: true,
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

const CONSTANT_BPM: Bpm = unsafe { Bpm::new_unchecked(MIDI_BASE_BPM.get()) };
const CONSTANT_DENOM: u32 = 4;

impl MidiSequence {
    pub fn new(
        ppq: u64,
        time_series: TimeSeries<MidiEventPayload>,
        time_info: MidiTimeInfo,
    ) -> Self {
        Self {
            ppq,
            time_series,
            time_info,
            normalized_second_to_pulse_factor: calculate_normalized_second_to_pulse_factor(ppq),
        }
    }

    pub fn empty(ppq: u64, capacity: usize, time_info: MidiTimeInfo) -> Self {
        Self::new(
            ppq,
            TimeSeries::new(Vec::with_capacity(capacity)),
            time_info,
        )
    }

    /// Parses a MIDI sequence from the given in-project MIDI REAPER chunk.
    pub fn parse_from_reaper_midi_chunk(chunk: &str) -> Result<Self, Box<dyn Error>> {
        let mut ppq = 960;
        let mut time_info = MidiTimeInfo::default();
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
                    ppq = e.ppq;
                }
                ReaperMidiChunkEntry::IgnTempo(d) => {
                    time_info = d;
                }
                ReaperMidiChunkEntry::Unhandled(_) => {}
            }
        }
        let sequence = MidiSequence::new(ppq, TimeSeries::new(events), time_info);
        Ok(sequence)
    }

    pub fn ppq(&self) -> u64 {
        self.ppq
    }

    pub fn time_series(&self) -> &TimeSeries<MidiEventPayload> {
        &self.time_series
    }

    pub fn time_info(&self) -> &MidiTimeInfo {
        &self.time_info
    }

    pub fn insert_event_at_normalized_midi_frame(
        &mut self,
        frame: usize,
        payload: MidiEventPayload,
    ) {
        let second = convert_duration_in_frames_to_seconds(frame, MIDI_FRAME_RATE);
        let pulse = self.convert_duration_in_seconds_to_pulse_normalized(second);
        self.insert_event_at_pulse(pulse, payload);
    }

    pub fn insert_event_at_pulse(&mut self, pulse_index: u64, payload: MidiEventPayload) {
        self.time_series.insert(pulse_index, payload);
    }

    /// Formats the sequence as REAPER in-project chunk.
    pub fn format_as_reaper_midi_chunk(&self) -> String {
        MidiSequenceAsReaperChunk(self).to_string()
    }

    /// Returns the total number of pulses of this sequence, in other words, the tempo-independent
    /// length of the sequence.
    ///
    /// This is not the number of events!
    pub fn total_pulse_count(&self) -> u64 {
        // The last pulse frame index marks the *exclusive* end, so we don't need + 1 here I guess.
        self.time_series.events.last().map(|e| e.frame).unwrap_or(0)
    }

    /// Calculates the number of MIDI frames according to [`MIDI_FRAME_RATE`] using a normalized
    /// tempo and time signature.
    fn calculate_midi_frame_count_normalized(&self, pulse_count: u64) -> usize {
        let seconds = pulse_count as f64 / self.normalized_second_to_pulse_factor;
        adjust_proportionally_positive(seconds, MIDI_FRAME_RATE.get())
    }

    fn convert_duration_in_seconds_to_pulse_normalized(&self, duration: DurationInSeconds) -> u64 {
        (duration.get() * self.normalized_second_to_pulse_factor).round() as u64
    }

    /// Calculates the length of this sequence given a particular tempo and time signature.
    pub fn calculate_length(&self, bpm: Bpm, time_sig_denominator: u32) -> DurationInSeconds {
        let second_to_pulse_factor =
            self.calculate_second_to_pulse_factor(bpm, time_sig_denominator);
        let length = self.total_pulse_count() as f64 / second_to_pulse_factor;
        DurationInSeconds::new(length)
    }

    fn calculate_second_to_pulse_factor(&self, bpm: Bpm, time_sig_denominator: u32) -> f64 {
        convert_second_to_pulse_flexible(1.0, bpm, time_sig_denominator, self.ppq)
    }
}

fn convert_pulse_to_second_flexible(
    pulse: u64,
    bpm: Bpm,
    time_sig_denominator: u32,
    ppq: u64,
) -> f64 {
    let bps = bpm.get() / 60.0;
    let duration_of_one_beat_in_secs = 1.0 / bps;
    let quarter_notes = pulse as f64 / ppq as f64;
    let beat = quarter_notes * get_qn_to_beat_factor(time_sig_denominator);
    beat * duration_of_one_beat_in_secs
}

fn calculate_normalized_second_to_pulse_factor(ppq: u64) -> f64 {
    convert_second_to_pulse_flexible(1.0, CONSTANT_BPM, CONSTANT_DENOM, ppq)
}

fn convert_second_to_pulse_flexible(
    second: f64,
    bpm: Bpm,
    time_sig_denominator: u32,
    ppq: u64,
) -> f64 {
    let bps = bpm.get() / 60.0;
    let duration_of_one_beat_in_secs = 1.0 / bps;
    let beat = second / duration_of_one_beat_in_secs;
    let quarter_notes = beat / get_qn_to_beat_factor(time_sig_denominator);
    quarter_notes * ppq as f64
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
            // Create aliases for brevity
            let frame_rate = request.dest_sample_rate;
            let start_frame = request.start_frame;
            let num_frames_to_be_consumed = request.dest_frame_count;
            // If sequence empty, stop right here
            let total_num_pulses = self.total_pulse_count();
            if total_num_pulses == 0 {
                return SupplyResponse::exceeded_end();
            }
            // If start frame behind end, stop right here
            let total_num_frames = self.calculate_midi_frame_count_normalized(total_num_pulses);
            if start_frame >= total_num_frames {
                return SupplyResponse::exceeded_end();
            };
            // Reduce number of requested frames according to that's still available
            let num_remaining_frames = total_num_frames - start_frame;
            let actual_num_frames_to_be_consumed =
                cmp::min(num_frames_to_be_consumed, num_remaining_frames);
            // Convert to pulses
            let start_time = convert_duration_in_frames_to_seconds(start_frame, frame_rate);
            let start_pulse = self.convert_duration_in_seconds_to_pulse_normalized(start_time);
            let actual_seconds_to_be_consumed =
                convert_duration_in_frames_to_seconds(actual_num_frames_to_be_consumed, frame_rate);
            let actual_num_pulses_to_be_consumed =
                self.convert_duration_in_seconds_to_pulse_normalized(actual_seconds_to_be_consumed);
            // Write events to event list
            let mut reaper_evt = reaper_medium::MidiEvent::default();
            let relevant_events = self
                .time_series
                .find_events_in_range(start_pulse, actual_num_pulses_to_be_consumed);
            let pulse_to_frame_factor = frame_rate.get() / self.normalized_second_to_pulse_factor;
            for evt in relevant_events {
                let pulse_offset = (evt.frame - start_pulse) as f64;
                let frame_offset = (pulse_offset * pulse_to_frame_factor).round() as u32;
                reaper_evt.set_frame_offset(MidiFrameOffset::new(frame_offset));
                reaper_evt.set_message(evt.payload.msg);
                event_list.add_item(&reaper_evt)
            }
            // TODO We don't include the last frame when it comes to reporting the pulse/frame count.
            //  Should we manually add this notes-off event? Or is it already transmitted? Or should we
            //  handle this via StartEndHandler?
            // The lower the sample rate, the higher the tempo, the more inner source material we
            // effectively grabbed.
            SupplyResponse::limited_by_total_frame_count(
                actual_num_frames_to_be_consumed,
                actual_num_frames_to_be_consumed,
                start_frame as isize,
                total_num_frames,
            )
        })
    }
}

impl WithMaterialInfo for MidiSequence {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        let info = MidiMaterialInfo {
            frame_count: self.calculate_midi_frame_count_normalized(self.total_pulse_count()),
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
            let igntempo = iter.next().ok_or("no IGNTEMPO flag")? == "1";
            let tempo: f64 = iter.next().ok_or("no custom tempo")?.parse()?;
            let numerator: u32 = iter.next().ok_or("no custom numerator")?.parse()?;
            let denominator: u32 = iter.next().ok_or("no custom denominator")?.parse()?;
            let data = MidiTimeInfo {
                igntempo,
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
        let midi_sequence = MidiSequence::new(
            960,
            TimeSeries::new(events),
            MidiTimeInfo {
                igntempo: true,
                tempo: Bpm::new(120.0).unwrap(),
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            },
        );
        // When
        assert_eq!(midi_sequence.total_pulse_count(), 960);
        let length_with_normal_tempo = midi_sequence.calculate_length(Bpm::new(120.0).unwrap(), 4);
        assert_eq!(length_with_normal_tempo.get(), 0.5);
        let length_with_different_time_sig =
            midi_sequence.calculate_length(Bpm::new(120.0).unwrap(), 8);
        assert_eq!(length_with_different_time_sig.get(), 1.0);
        let length_with_double_tempo = midi_sequence.calculate_length(Bpm::new(240.0).unwrap(), 4);
        assert_eq!(length_with_double_tempo.get(), 0.25);
        let length_with_half_tempo = midi_sequence.calculate_length(Bpm::new(60.0).unwrap(), 4);
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
                igntempo: true,
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
        let midi_sequence = MidiSequence::new(
            960,
            TimeSeries::new(events),
            MidiTimeInfo {
                igntempo: true,
                tempo: Bpm::new(120.0).unwrap(),
                time_signature: TimeSignature {
                    numerator: 4,
                    denominator: 4,
                },
            },
        );
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
