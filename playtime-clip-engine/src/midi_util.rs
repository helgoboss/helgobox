use helgoboss_midi::ShortMessage;

pub fn is_play_message(msg: &impl ShortMessage) -> bool {
    msg.is_note_on()
}
