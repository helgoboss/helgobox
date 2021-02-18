use crate::domain::{CompoundMappingSource, MappingId};
use helgoboss_learn::{MidiSource, OscSource};
use std::collections::HashSet;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent {
    LearnedSource {
        source: RealSource,
        allow_virtual_sources: bool,
    },
    UpdatedOnMappings(HashSet<MappingId>),
}

pub trait DomainEventHandler: Debug {
    fn handle_event(&self, event: DomainEvent);
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub enum RealSource {
    Midi(MidiSource),
    Osc(OscSource),
}

impl RealSource {
    pub fn into_compound_source(self) -> CompoundMappingSource {
        use RealSource::*;
        match self {
            Midi(s) => CompoundMappingSource::Midi(s),
            Osc(s) => CompoundMappingSource::Osc(s),
        }
    }

    pub fn from_compound_source(s: CompoundMappingSource) -> Option<Self> {
        use CompoundMappingSource::*;
        match s {
            Midi(s) => Some(Self::Midi(s)),
            Osc(s) => Some(Self::Osc(s)),
            Virtual(_) => None,
        }
    }
}
