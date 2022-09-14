use crate::rt::supplier::MidiSupplier;
use helgoboss_midi::{controller_numbers, Channel, RawShortMessage, ShortMessageFactory, U7};
use playtime_api::persistence::MidiResetMessages;
use reaper_medium::{BorrowedMidiEventList, MidiEvent, MidiFrameOffset};

pub fn silence_midi(
    event_list: &mut BorrowedMidiEventList,
    reset_messages: MidiResetMessages,
    block_mode: SilenceMidiBlockMode,
    supplier: &mut dyn MidiSupplier,
) {
    if !reset_messages.at_least_one_enabled() {
        return;
    }
    use SilenceMidiBlockMode::*;
    let frame_offset = match block_mode {
        Prepend => {
            for evt in event_list.iter_mut() {
                if evt.frame_offset() == MidiFrameOffset::MIN {
                    evt.set_frame_offset(MidiFrameOffset::new(1));
                }
            }
            MidiFrameOffset::MIN
        }
        Append => event_list
            .iter()
            .map(|evt| evt.frame_offset())
            .max()
            .map(|o| MidiFrameOffset::new(o.get() + 1))
            .unwrap_or(MidiFrameOffset::MIN),
    };
    // At the moment, resetting on-notes is only supported as append (= at the end). If we would ask
    // to release notes when prepending (at the start), we would need to make sure that the
    // to-be-released notes are captured *before* filling the event list. But this function is
    // usually called *after* filling the event list.
    if reset_messages.on_notes_off && block_mode == Append {
        supplier.release_notes(frame_offset, event_list);
    }
    for ch in 0..16 {
        let mut append_reset = |cc| {
            let msg = RawShortMessage::control_change(Channel::new(ch), cc, U7::MIN);
            add_midi_event(event_list, frame_offset, msg);
        };
        if reset_messages.all_notes_off {
            append_reset(controller_numbers::ALL_NOTES_OFF);
        }
        if reset_messages.all_sound_off {
            append_reset(controller_numbers::ALL_SOUND_OFF);
        }
        if reset_messages.reset_all_controllers {
            append_reset(controller_numbers::RESET_ALL_CONTROLLERS);
        }
        if reset_messages.damper_pedal_off {
            append_reset(controller_numbers::DAMPER_PEDAL_ON_OFF);
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum SilenceMidiBlockMode {
    Prepend,
    Append,
}

fn add_midi_event(
    event_list: &mut BorrowedMidiEventList,
    frame_offset: MidiFrameOffset,
    msg: RawShortMessage,
) {
    let mut event = MidiEvent::default();
    event.set_frame_offset(frame_offset);
    event.set_message(msg);
    event_list.add_item(&event);
}
