use crate::domain::{
    ClipChangedEvent, CompoundMappingTarget, MappingCompartment, MappingId, MessageCaptureResult,
    ParameterArray, ProjectionFeedbackValue, QualifiedMappingId,
};
use helgoboss_learn::AbsoluteValue;
use std::collections::HashSet;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent<'a> {
    CapturedIncomingMessage(MessageCaptureEvent),
    UpdatedOnMappings(HashSet<QualifiedMappingId>),
    UpdatedSingleMappingOnState(UpdatedSingleMappingOnStateEvent),
    UpdatedParameter { index: u32, value: f32 },
    UpdatedAllParameters(Box<ParameterArray>),
    TargetValueChanged(TargetValueChangedEvent<'a>),
    ProjectionFeedback(ProjectionFeedbackValue),
    MappingMatched(MappingMatchedEvent),
    FullResyncRequested,
    MappingEnabledChangeRequested(MappingEnabledChangeRequestedEvent),
    ClipSlotsUpdated(Vec<ClipSlotUpdatedEvent>),
}

#[derive(Copy, Clone, Debug)]
pub struct ClipSlotUpdatedEvent {
    pub slot_index: usize,
    pub clip_changed_event: ClipChangedEvent,
}

#[derive(Clone, Debug)]
pub struct MessageCaptureEvent {
    pub result: MessageCaptureResult,
    pub allow_virtual_sources: bool,
    pub osc_arg_index_hint: Option<u32>,
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

    fn notify_mapping_matched(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        self.handle_event(DomainEvent::MappingMatched(MappingMatchedEvent::new(
            compartment,
            mapping_id,
        )));
    }
}
