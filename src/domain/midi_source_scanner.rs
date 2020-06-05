use helgoboss_learn::{MidiSource, MidiSourceValue};
use helgoboss_midi::ShortMessage;

#[derive(Default)]
pub struct MidiSourceScanner {}

impl MidiSourceScanner {
    pub fn feed(&mut self, source_value: MidiSourceValue<impl ShortMessage>) -> Option<MidiSource> {
        todo!()
    }

    pub fn reset(&mut self) {
        todo!()
    }
}
