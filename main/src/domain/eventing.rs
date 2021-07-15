use crate::domain::{
    CompoundMappingSource, CompoundMappingTarget, MappingCompartment, MappingId, MidiSource,
    ParameterArray, ProjectionFeedbackValue, SourceFeedbackValue,
};
use helgoboss_learn::{AbsoluteValue, OscSource};
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
    ProjectionFeedback(ProjectionFeedbackValue),
    FullResyncRequested,
}

#[derive(Debug)]
pub struct TargetValueChangedEvent<'a> {
    pub compartment: MappingCompartment,
    pub mapping_id: MappingId,
    pub targets: &'a [CompoundMappingTarget],
    pub new_value: AbsoluteValue,
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
            Virtual(_) | Never => None,
        }
    }

    pub fn from_feedback_value(value: &SourceFeedbackValue) -> Option<Self> {
        use SourceFeedbackValue::*;
        match value {
            Midi(v) => MidiSource::from_source_value(v.clone()).map(Self::Midi),
            Osc(v) => Some(Self::Osc(OscSource::from_source_value(v.clone(), None))),
        }
    }
}
