use crate::conversion_util::{
    adjust_proportionally_positive, convert_duration_in_frames_to_seconds,
    convert_duration_in_seconds_to_frames, convert_position_in_frames_to_seconds,
};
use crate::rt::buffer::AudioBufMut;
use crate::rt::source_util::pcm_source_is_midi;
use crate::rt::supplier::audio_util::{supply_audio_material, SourceMaterialRequest};
use crate::rt::supplier::log_util::print_distance_from_beat_start_at;
use crate::rt::supplier::midi_sequence::MidiSequence;
use crate::rt::supplier::{
    AudioMaterialInfo, AudioSupplier, MaterialInfo, MidiMaterialInfo, MidiSilencer, MidiSupplier,
    PositionTranslationSkill, SupplyAudioRequest, SupplyMidiRequest, SupplyResponse,
    WithMaterialInfo, WithSource,
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
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct MidiNoteTracker<S> {
    midi_state: MidiState,
    supplier: S,
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
            .map(KeyNumber::new)
    }

    fn note_is_on(&self, note: u8) -> bool {
        (self.0 & (1 << note)) > 0
    }
}

impl<S> MidiNoteTracker<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            supplier,
            midi_state: MidiState::default(),
        }
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }
}

impl<S: AudioSupplier> AudioSupplier for MidiNoteTracker<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        self.supplier.supply_audio(request, dest_buffer)
    }
}

impl<S: WithMaterialInfo> WithMaterialInfo for MidiNoteTracker<S> {
    fn material_info(&self) -> ClipEngineResult<MaterialInfo> {
        self.supplier.material_info()
    }
}

impl<S: MidiSupplier> MidiSupplier for MidiNoteTracker<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &mut BorrowedMidiEventList,
    ) -> SupplyResponse {
        let response = self.supplier.supply_midi(request, event_list);
        // Track playing notes
        for evt in event_list {
            self.midi_state.update(evt.message())
        }
        response
    }
}

impl<S: Debug> MidiSilencer for MidiNoteTracker<S> {
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

impl<S: PositionTranslationSkill> PositionTranslationSkill for MidiNoteTracker<S> {
    fn translate_play_pos_to_source_pos(&self, play_pos: isize) -> isize {
        self.supplier.translate_play_pos_to_source_pos(play_pos)
    }
}

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
