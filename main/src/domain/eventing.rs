use crate::domain::{CompoundMappingSource, MappingId};
use std::collections::HashSet;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource(CompoundMappingSource),
    UpdateOnMappings(HashSet<MappingId>),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}
