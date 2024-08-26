use helgoboss_learn::RawMidiEvent;

#[derive(Debug)]
pub struct MidiTransformationContainer {
    events: Vec<RawMidiEvent>,
}

impl MidiTransformationContainer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, event: RawMidiEvent) {
        self.events.push(event);
    }

    pub fn drain(&mut self) -> impl Iterator<Item = RawMidiEvent> + '_ {
        self.events.drain(..)
    }
}
