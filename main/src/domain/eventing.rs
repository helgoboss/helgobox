use crate::domain::MappingId;
use helgoboss_learn::MidiSource;
use std::collections::HashSet;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource {
        source: MidiSource,
        allow_virtual_sources: bool,
    },
    UpdatedOnMappings(HashSet<MappingId>),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}
