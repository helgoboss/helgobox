use crate::domain::NormalMappingSource;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource(NormalMappingSource),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}
