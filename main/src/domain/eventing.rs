use crate::domain::{
    CompartmentKind, CompoundMappingTarget, ControlLogContext, ControlLogEntry, FeedbackLogEntry,
    InternalInfoEvent, MappingId, MessageCaptureResult, PluginParamIndex, PluginParams,
    ProjectionFeedbackValue, QualifiedMappingId, RawParamValue,
};
use base::hash_util::NonCryptoHashSet;
use helgoboss_learn::{AbsoluteValue, ControlValue};
use helgobox_api::persistence::MappingModification;
use std::error::Error;
use std::fmt::Debug;

/// An event which is sent to upper layers and processed there
#[derive(Debug)]
pub enum DomainEvent<'a> {
    CapturedIncomingMessage(MessageCaptureEvent),
    GlobalControlAndFeedbackStateChanged(GlobalControlAndFeedbackState),
    UpdatedOnMappings(NonCryptoHashSet<QualifiedMappingId>),
    UpdatedSingleMappingOnState(UpdatedSingleMappingOnStateEvent),
    UpdatedSingleParameterValue {
        index: PluginParamIndex,
        value: RawParamValue,
    },
    UpdatedAllParameters(PluginParams),
    TargetValueChanged(TargetValueChangedEvent<'a>),
    Info(&'a InternalInfoEvent),
    ProjectionFeedback(ProjectionFeedbackValue),
    MappingMatched(MappingMatchedEvent),
    HandleTargetControl(TargetControlEvent),
    HandleSourceFeedback(SourceFeedbackEvent<'a>),
    FullResyncRequested,
    MidiDevicesChanged,
    MappingEnabledChangeRequested(MappingEnabledChangeRequestedEvent),
    MappingModificationRequested(MappingModificationRequestedEvent),
    TimeForCelebratingSuccess,
    ConditionsChanged,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct GlobalControlAndFeedbackState {
    pub control_active: bool,
    pub feedback_active: bool,
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
    pub compartment: CompartmentKind,
    pub mapping_id: MappingId,
    pub is_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct MappingModificationRequestedEvent {
    pub compartment: CompartmentKind,
    pub mapping_id: MappingId,
    pub modification: MappingModification,
    pub value: ControlValue,
}

#[derive(Copy, Clone, Debug)]
pub struct MappingMatchedEvent {
    pub compartment: CompartmentKind,
    pub mapping_id: MappingId,
}

impl MappingMatchedEvent {
    pub fn new(compartment: CompartmentKind, mapping_id: MappingId) -> Self {
        MappingMatchedEvent {
            compartment,
            mapping_id,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TargetControlEvent {
    pub id: QualifiedMappingId,
    pub log_context: ControlLogContext,
    pub log_entry: ControlLogEntry,
}

#[derive(Copy, Clone, Debug)]
pub struct SourceFeedbackEvent<'a> {
    pub id: QualifiedMappingId,
    pub log_entry: FeedbackLogEntry<'a>,
}

impl TargetControlEvent {
    pub fn new(
        id: QualifiedMappingId,
        log_context: ControlLogContext,
        log_entry: ControlLogEntry,
    ) -> Self {
        Self {
            id,
            log_context,
            log_entry,
        }
    }
}

#[derive(Debug)]
pub struct TargetValueChangedEvent<'a> {
    pub compartment: CompartmentKind,
    pub mapping_id: MappingId,
    pub targets: &'a [CompoundMappingTarget],
    pub new_value: AbsoluteValue,
}

pub trait DomainEventHandler: Debug {
    fn handle_event_ignoring_error(&self, event: DomainEvent) {
        let _ = self.handle_event(event);
    }

    fn handle_event(&self, event: DomainEvent) -> Result<(), Box<dyn Error>>;

    fn notify_mapping_matched(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        self.handle_event_ignoring_error(DomainEvent::MappingMatched(MappingMatchedEvent::new(
            compartment,
            mapping_id,
        )));
    }

    /// Returns `true` if another preset is being loaded.
    fn auto_load_different_preset_if_necessary(&self) -> anyhow::Result<bool>;
}
