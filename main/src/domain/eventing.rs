use helgoboss_learn::MidiSource;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource(MidiSource),
}
