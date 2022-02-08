use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use reaper_medium::{BorrowedMidiEventList, MidiEvent};

pub fn silence_midi(event_list: &BorrowedMidiEventList) {
    for ch in 0..16 {
        let all_notes_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_NOTES_OFF,
            U7::MIN,
        );
        let all_sound_off = RawShortMessage::control_change(
            Channel::new(ch),
            controller_numbers::ALL_SOUND_OFF,
            U7::MIN,
        );
        add_midi_event(event_list, all_notes_off);
        add_midi_event(event_list, all_sound_off);
    }
}

fn add_midi_event(event_list: &BorrowedMidiEventList, msg: RawShortMessage) {
    let mut event = MidiEvent::default();
    event.set_message(msg);
    event_list.add_item(&event);
}
