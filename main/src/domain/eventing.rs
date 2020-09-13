use crate::domain::CompoundMappingSource;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource(CompoundMappingSource),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}
