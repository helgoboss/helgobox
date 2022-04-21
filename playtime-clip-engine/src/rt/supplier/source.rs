use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_seconds,
    convert_duration_in_seconds_to_frames, convert_position_in_frames_to_seconds,
};
use crate::rt::buffer::AudioBufMut;
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, SourceMaterialRequest};
use crate::rt::supplier::log_util::print_distance_from_beat_start_at;
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiMaterialInfo, MidiSupplier,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithMaterialInfo, WithSource,
};
use crate::ClipEngineResult;
use helgoboss_midi::{
    Channel, KeyNumber, RawShortMessage, ShortMessage, ShortMessageFactory, StructuredShortMessage,
    U7,
};
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, Bpm, DurationInSeconds, Hz, MidiEvent,
    MidiFrameOffset, OwnedPcmSource, PcmSourceTransfer,
};

#[derive(Clone, Debug)]
pub struct ClipSource {
    source: OwnedPcmSource,
    midi_state: MidiState,
}

#[derive(Clone, Debug, Default)]
struct MidiState {
    note_states_by_channel: [NoteState; 16],
}

impl MidiState {
    pub fn reset(&mut self) {
        for state in &mut self.note_states_by_channel {
            state.reset();
        }
    }

    pub fn update(&mut self, msg: &impl ShortMessage) {
        match msg.to_structured() {
            StructuredShortMessage::NoteOn {
                channel,
                key_number,
                velocity,
            } => {
                let state = &mut self.note_states_by_channel[channel.get() as usize];
                if velocity.get() > 0 {
                    state.add_note(key_number);
                } else {
                    state.remove_note(key_number);
                }
            }
            StructuredShortMessage::NoteOff {
                channel,
                key_number,
                ..
            } => {
                self.note_states_by_channel[channel.get() as usize].remove_note(key_number);
            }
            _ => {}
        }
    }

    pub fn on_notes(&self) -> impl Iterator<Item = (Channel, KeyNumber)> + '_ {
        self.note_states_by_channel
            .iter()
            .enumerate()
            .flat_map(|(ch, note_state)| {
                let ch = Channel::new(ch as _);
                note_state.on_notes().map(move |note| (ch, note))
            })
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Default, Debug)]
struct NoteState(u128);

impl NoteState {
    pub fn reset(&mut self) {
        self.0 = 0;
    }

    pub fn add_note(&mut self, note: KeyNumber) {
        self.0 |= 1 << (note.get() as u128);
    }

    pub fn remove_note(&mut self, note: KeyNumber) {
        self.0 &= !(1 << (note.get() as u128));
    }

    pub fn on_notes(&self) -> impl Iterator<Item = KeyNumber> + '_ {
        (0u8..128u8)
            .filter(|note| self.note_is_on(*note))
            .map(|note| KeyNumber::new(note))
    }

    fn note_is_on(&self, note: u8) -> bool {
        (self.0 & (1 << note)) > 0
    }
}

impl ClipSource {
    pub fn new(reaper_source: OwnedPcmSource) -> Self {
        Self {
            source: reaper_source,
            midi_state: MidiState::default(),
        }
    }

    pub fn reaper_source(&self) -> &BorrowedPcmSource {
        &self.source
    }

    pub fn into_reaper_source(self) -> OwnedPcmSource {
        self.source
    }

    fn get_audio_source_frame_rate(&self) -> Hz {
        self.source
            .get_sample_rate()
            .expect("audio source should expose frame rate")
    }

    fn transfer_audio(&self, req: SourceMaterialRequest) -> SupplyResponse {
        let source_sample_rate = self.source.get_sample_rate().unwrap();
        let time_s = convert_duration_in_frames_to_seconds(req.start_frame, source_sample_rate);
        let num_frames_written = unsafe {
            let mut transfer = PcmSourceTransfer::default();
            // Both channel count and sample rate should be the one from the source itself!
            transfer.set_nch(self.get_audio_source_channel_count() as _);
            transfer.set_sample_rate(source_sample_rate);
            // The rest depends on the given parameters
            transfer.set_length(req.dest_buffer.frame_count() as _);
            transfer.set_samples(req.dest_buffer.data_as_mut_ptr());
            transfer.set_time_s(time_s.into());
            self.source.get_samples(&transfer);
            transfer.samples_out() as usize
        };
        // The lower the sample rate, the higher the tempo, the more inner source material we
        // effectively grabbed.
        SupplyResponse::limited_by_total_frame_count(
            num_frames_written,
            num_frames_written,
            req.start_frame as isize,
            self.calculate_audio_frame_count(source_sample_rate),
        )
    }

    fn get_audio_source_channel_count(&self) -> usize {
        self.source
            .get_num_channels()
            .expect("audio source should report channel count") as usize
    }

    fn calculate_audio_frame_count(&self, sample_rate: Hz) -> usize {
        let length_in_seconds = self.source.get_length().unwrap_or(DurationInSeconds::ZERO);
        convert_duration_in_seconds_to_frames(length_in_seconds, sample_rate)
    }

    fn calculate_midi_frame_count(&self) -> usize {
        let length_in_seconds = if let Some(length_in_beats) = self.source.get_length_beats() {
            // For MIDI, get_length() takes the current project tempo in account ... which is not
            // what we want because we want to do all the tempo calculations ourselves and treat
            // MIDI/audio the same wherever possible.
            let beats_per_minute = MIDI_BASE_BPM;
            let beats_per_second = beats_per_minute.get() / 60.0;
            DurationInSeconds::new(length_in_beats.get() / beats_per_second)
        } else {
            // If we don't get a length in beats, this either means we have set a preview tempo
            // on the source or the source has IGNTEMPO set to 1. Either way we will take the
            // reported length.
            self.source.get_length().unwrap()
        };
        convert_duration_in_seconds_to_frames(length_in_seconds, MIDI_FRAME_RATE)
    }
}

impl AudioSupplier for ClipSource {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let source_frame_rate = self.get_audio_source_frame_rate();
        supply_audio_material(request, dest_buffer, source_frame_rate, |input| {
            self.transfer_audio(input)
        })
    }
}

impl WithMaterialInfo for ClipSource {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        let info = if pcm_source_is_midi(&self.source) {
            let info = MidiMaterialInfo {
                frame_count: self.calculate_midi_frame_count(),
            };
            MaterialInfo::Midi(info)
        } else {
            let sample_rate = self.get_audio_source_frame_rate();
            let info = AudioMaterialInfo {
                channel_count: self.get_audio_source_channel_count(),
                frame_count: self.calculate_audio_frame_count(sample_rate),
                frame_rate: sample_rate,
            };
            MaterialInfo::Audio(info)
        };
        Ok(info)
    }
}

impl MidiSupplier for ClipSource {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        // This logic assumes that the destination frame rate is comparable to the source frame
        // rate. The resampler makes sure of it. However, it's not necessarily equal since we use
        // frame rate changes for tempo changes. It's only equal if the clip is played in
        // MIDI_BASE_BPM.
        let frame_rate = request.dest_sample_rate;
        let num_frames_to_be_consumed = request.dest_frame_count;
        if request.start_frame == 0 {
            print_distance_from_beat_start_at(request, 0, "(MIDI, start_frame = 0)");
        } else if request.start_frame < 0
            && (request.start_frame + num_frames_to_be_consumed as isize) >= 0
        {
            let distance_to_zero_in_midi_frames = (-request.start_frame) as usize;
            let ratio = request.dest_frame_count as f64 / num_frames_to_be_consumed as f64;
            let distance_to_zero_in_dest_frames =
                adjust_proportionally_positive(distance_to_zero_in_midi_frames as f64, ratio);
            print_distance_from_beat_start_at(
                request,
                distance_to_zero_in_dest_frames,
                "(MIDI, start_frame < 0)",
            );
        }
        // For MIDI it seems to be okay to start at a negative position. The source
        // will ignore positions < 0.0 and add events >= 0.0 with the correct frame
        // offset.
        let time_s = convert_position_in_frames_to_seconds(request.start_frame, frame_rate);
        let num_midi_frames_consumed = unsafe {
            let mut transfer = PcmSourceTransfer::default();
            transfer.set_sample_rate(frame_rate);
            transfer.set_length(num_frames_to_be_consumed as i32);
            transfer.set_time_s(time_s);
            transfer.set_midi_event_list(event_list);
            self.source.get_samples(&transfer);
            // In the past, we did the following in order to deal with on-the-fly tempo changes that
            // occur while playing instead of REAPER letting use its generic mechanism that leads
            // to repeated notes, probably through internal position changes.
            //
            //      transfer.set_force_bpm(MIDI_BASE_BPM);
            //      transfer.set_absolute_time_s(PositionInSeconds::ZERO);
            //
            // However, now we set the constant preview tempo at source creation time, which makes
            // the source completely project tempo/pos-independent, also when doing recording via
            // midi_realtime_write_struct_t. So that's not necessary anymore
            transfer.samples_out() as usize
        };
        // Track playing notes
        for evt in event_list {
            self.midi_state.update(evt.message())
        }
        // The lower the sample rate, the higher the tempo, the more inner source material we
        // effectively grabbed.
        SupplyResponse::limited_by_total_frame_count(
            num_midi_frames_consumed,
            num_midi_frames_consumed,
            request.start_frame,
            self.calculate_midi_frame_count(),
        )
    }

    fn release_notes(
        &mut self,
        frame_offset: MidiFrameOffset,
        event_list: &mut BorrowedMidiEventList,
    ) {
        for (ch, note) in self.midi_state.on_notes() {
            let msg = RawShortMessage::note_off(ch, note, U7::MIN);
            let mut event = MidiEvent::default();
            event.set_frame_offset(frame_offset);
            event.set_message(msg);
            event_list.add_item(&event);
        }
        self.midi_state.reset();
    }
}

impl WithSource for ClipSource {
    fn source(&self) -> Option<&ClipSource> {
        Some(self)
    }
}

/// We could use just any unit to represent a position within a MIDI source, but we choose frames
/// with regard to the following frame rate. Choosing frames allows us to treat MIDI similar to
/// audio, which results in fewer special cases. The frame rate of 169,344,000 is a multiple of
/// all common sample rates and PPQs. This prevents rounding issues (advice from Justin).
/// Initially I wanted to take 1,024,000 because it is the unit which is used in REAPER's MIDI
/// events, but it's not a multiple of common sample rates and PPQs.
pub const MIDI_FRAME_RATE: Hz = unsafe { Hz::new_unchecked(169_344_000.0) };

/// MIDI data is tempo-less. But pretending that all MIDI clips have a fixed tempo allows us to
/// treat MIDI similar to audio. E.g. if we want it to play faster, we just lower the output sample
/// rate. Plus, we can use the same time stretching supplier. Fewer special cases, nice!
pub const MIDI_BASE_BPM: Bpm = unsafe { Bpm::new_unchecked(120.0) };

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::test_util::*;

    #[test]
    fn note_state_basics() {
        // Given
        let mut note_state = NoteState::default();
        // When
        note_state.add_note(KeyNumber::new(5));
        note_state.add_note(KeyNumber::new(7));
        note_state.add_note(KeyNumber::new(105));
        // Then
        assert!(!note_state.note_is_on(0));
        assert!(note_state.note_is_on(5));
        assert!(note_state.note_is_on(7));
        assert!(!note_state.note_is_on(100));
        assert!(note_state.note_is_on(105));
        let on_notes: Vec<_> = note_state.on_notes().collect();
        assert_eq!(
            on_notes,
            vec![KeyNumber::new(5), KeyNumber::new(7), KeyNumber::new(105)]
        );
    }

    #[test]
    fn note_state_remove() {
        // Given
        let mut note_state = NoteState::default();
        // When
        note_state.add_note(key_number(5));
        note_state.add_note(key_number(7));
        note_state.remove_note(key_number(5));
        note_state.add_note(key_number(105));
        note_state.remove_note(key_number(105));
        // Then
        assert!(!note_state.note_is_on(0));
        assert!(!note_state.note_is_on(5));
        assert!(note_state.note_is_on(7));
        assert!(!note_state.note_is_on(100));
        let on_notes: Vec<_> = note_state.on_notes().collect();
        assert_eq!(on_notes, vec![key_number(7)]);
    }

    #[test]
    fn midi_state_update() {
        // Given
        let mut midi_state = MidiState::default();
        // When
        midi_state.update(&note_on(0, 7, 100));
        midi_state.update(&note_on(0, 120, 120));
        midi_state.update(&note_on(0, 5, 120));
        midi_state.update(&note_on(0, 7, 0));
        midi_state.update(&note_on(0, 120, 1));
        midi_state.update(&note_off(0, 5, 20));
        // Then
        let on_notes: Vec<_> = midi_state.on_notes().collect();
        assert_eq!(on_notes, vec![(channel(0), key_number(120))]);
    }

    #[test]
    fn midi_state_reset() {
        // Given
        let mut midi_state = MidiState::default();
        // When
        midi_state.update(&note_on(0, 7, 100));
        midi_state.update(&note_on(0, 120, 120));
        midi_state.update(&note_on(0, 5, 120));
        midi_state.update(&note_on(0, 7, 0));
        midi_state.update(&note_on(5, 120, 1));
        midi_state.update(&note_off(7, 5, 20));
        midi_state.reset();
        // Then
        let on_notes: Vec<_> = midi_state.on_notes().collect();
        assert_eq!(on_notes, vec![]);
    }

    #[test]
    fn midi_state_update_with_channels() {
        // Given
        let mut midi_state = MidiState::default();
        // When
        midi_state.update(&note_on(0, 7, 100));
        midi_state.update(&note_on(0, 120, 120));
        midi_state.update(&note_on(0, 5, 120));
        midi_state.update(&note_on(1, 7, 0));
        midi_state.update(&note_on(3, 7, 50));
        midi_state.update(&note_on(0, 120, 1));
        midi_state.update(&note_off(0, 5, 20));
        // Then
        let on_notes: Vec<_> = midi_state.on_notes().collect();
        assert_eq!(
            on_notes,
            vec![
                (channel(0), key_number(7)),
                (channel(0), key_number(120)),
                (channel(3), key_number(7))
            ]
        );
    }
}
