





use crate::rt::supplier::{
    AutoDelegatingAudioSupplier,
    AutoDelegatingPositionTranslationSkill, AutoDelegatingWithMaterialInfo, MidiSilencer, MidiSupplier,
    SupplyMidiRequest, SupplyResponse, WithSupplier,
};

use helgoboss_midi::{
    Channel, KeyNumber, RawShortMessage, ShortMessage, ShortMessageFactory, StructuredShortMessage,
    U7,
};
use reaper_medium::{
    BorrowedMidiEventList, MidiEvent,
    MidiFrameOffset,
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
}

impl<S> WithSupplier for MidiNoteTracker<S> {
    type Supplier = S;

    fn supplier(&self) -> &Self::Supplier {
        &self.supplier
    }

    fn supplier_mut(&mut self) -> &mut Self::Supplier {
        &mut self.supplier
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

impl<S> AutoDelegatingAudioSupplier for MidiNoteTracker<S> {}
impl<S> AutoDelegatingWithMaterialInfo for MidiNoteTracker<S> {}
impl<S> AutoDelegatingPositionTranslationSkill for MidiNoteTracker<S> {}

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
