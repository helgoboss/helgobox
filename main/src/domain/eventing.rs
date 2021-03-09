use crate::domain::{
    CompoundMappingSource, CompoundMappingTarget, MappingCompartment, MappingId, ParameterArray,
};
use helgoboss_learn::{MidiSource, OscSource, UnitValue};
use std::collections::HashSet;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent<'a> {
    LearnedSource {
        source: RealSource,
        allow_virtual_sources: bool,
    },
    UpdatedOnMappings(HashSet<MappingId>),
    UpdatedParameter {
        index: u32,
        value: f32,
    },
    UpdatedAllParameters(Box<ParameterArray>),
    TargetValueChanged(TargetValueChangedEvent<'a>),
    FullResyncRequested,
}

#[derive(Debug)]
pub struct TargetValueChangedEvent<'a> {
    pub compartment: MappingCompartment,
    pub mapping_id: MappingId,
    pub target: Option<&'a CompoundMappingTarget>,
    pub new_value: UnitValue,
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
