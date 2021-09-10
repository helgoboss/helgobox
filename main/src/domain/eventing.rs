use crate::domain::{
    CompoundMappingSource, CompoundMappingTarget, MappingCompartment, MappingId, MidiSource,
    ParameterArray, ProjectionFeedbackValue, QualifiedMappingId, ReaperSource, SourceFeedbackValue,
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
    UpdatedOnMappings(HashSet<QualifiedMappingId>),
    UpdatedSingleMappingOnState(UpdatedSingleMappingOnStateEvent),
    UpdatedParameter {
        index: u32,
        value: f32,
    },
    UpdatedAllParameters(Box<ParameterArray>),
    TargetValueChanged(TargetValueChangedEvent<'a>),
    ProjectionFeedback(ProjectionFeedbackValue),
    MappingMatched(MappingMatchedEvent),
    FullResyncRequested,
    MappingEnabledChangeRequested(MappingEnabledChangeRequestedEvent),
}

#[derive(Copy, Clone, Debug)]
pub struct UpdatedSingleMappingOnStateEvent {
    pub id: QualifiedMappingId,
    pub is_on: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct MappingEnabledChangeRequestedEvent {
    pub compartment: MappingCompartment,
    pub mapping_id: MappingId,
    pub is_enabled: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct MappingMatchedEvent {
    pub compartment: MappingCompartment,
    pub mapping_id: MappingId,
}

impl MappingMatchedEvent {
    pub fn new(compartment: MappingCompartment, mapping_id: MappingId) -> Self {
        MappingMatchedEvent {
            compartment,
            mapping_id,
        }
    }
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
    Reaper(ReaperSource),
}

impl RealSource {
    pub fn into_compound_source(self) -> CompoundMappingSource {
        use RealSource::*;
        match self {
            Midi(s) => CompoundMappingSource::Midi(s),
            Osc(s) => CompoundMappingSource::Osc(s),
            Reaper(s) => CompoundMappingSource::Reaper(s),
        }
    }

    pub fn from_compound_source(s: CompoundMappingSource) -> Option<Self> {
        use CompoundMappingSource::*;
        match s {
            Midi(s) => Some(Self::Midi(s)),
            Osc(s) => Some(Self::Osc(s)),
            Reaper(s) => Some(Self::Reaper(s)),
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
