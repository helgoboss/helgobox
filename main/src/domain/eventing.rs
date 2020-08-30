use helgoboss_learn::MidiSource;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource(MidiSource),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}
