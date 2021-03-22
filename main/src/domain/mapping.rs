use crate::domain::{
    ActivationChange, ActivationCondition, ControlOptions, ExtendedProcessorContext,
    MappingActivationEffect, Mode, ParameterArray, PlayPosFeedbackResolution, RealearnTarget,
    ReaperTarget, TargetCharacter, UnresolvedReaperTarget, VirtualControlElement, VirtualSource,
    VirtualSourceValue, VirtualTarget,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use helgoboss_learn::{
    ControlType, ControlValue, MidiSource, MidiSourceValue, OscSource, RawMidiEvent,
    SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{RawShortMessage, ShortMessage};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use rosc::OscMessage;
use serde::{Deserialize, Serialize};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::fmt::Display;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct ProcessorMappingOptions {
    pub target_is_active: bool,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub prevent_echo_feedback: bool,
    pub send_feedback_after_control: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MappingId {
    uuid: Uuid,
}

impl MappingId {
    pub fn random() -> MappingId {
        MappingId {
            uuid: Uuid::new_v4(),
        }
    }
}

impl Display for MappingId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

const MAX_ECHO_FEEDBACK_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum LifecycleMidiMessage {
    #[allow(unused)]
    Short(RawShortMessage),
    Raw(Box<RawMidiEvent>),
}

#[derive(Debug, Default)]
pub struct LifecycleMidiData {
    pub activation_midi_messages: Vec<LifecycleMidiMessage>,
    pub deactivation_midi_messages: Vec<LifecycleMidiMessage>,
}

#[derive(Debug, Default)]
pub struct MappingExtension {
    /// If it's None, it means it's splintered already.
    lifecycle_midi_data: Option<LifecycleMidiData>,
}

impl MappingExtension {
    pub fn new(lifecycle_midi_data: LifecycleMidiData) -> Self {
        Self {
            lifecycle_midi_data: Some(lifecycle_midi_data),
        }
    }
}

// TODO-low The name is confusing. It should be MainThreadMapping or something because
//  this can also be a controller mapping (a mapping in the controller compartment).
#[derive(Debug)]
pub struct MainMapping {
    core: MappingCore,
    unresolved_target: Option<UnresolvedCompoundMappingTarget>,
    activation_condition_1: ActivationCondition,
    activation_condition_2: ActivationCondition,
    is_active_1: bool,
    is_active_2: bool,
    extension: MappingExtension,
}

impl MainMapping {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        compartment: MappingCompartment,
        id: MappingId,
        source: CompoundMappingSource,
        mode: Mode,
        unresolved_target: Option<UnresolvedCompoundMappingTarget>,
        activation_condition_1: ActivationCondition,
        activation_condition_2: ActivationCondition,
        options: ProcessorMappingOptions,
        extension: MappingExtension,
    ) -> MainMapping {
        MainMapping {
            core: MappingCore {
                compartment,
                id,
                source,
                mode,
                target: None,
                options,
                time_of_last_control: None,
            },
            unresolved_target,
            activation_condition_1,
            activation_condition_2,
            is_active_1: false,
            is_active_2: false,
            extension,
        }
    }

    pub fn qualified_source(&self) -> QualifiedSource {
        QualifiedSource {
            compartment: self.core.compartment,
            id: self.id(),
            source: self.source().clone(),
        }
    }

    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn splinter_real_time_mapping(&mut self) -> RealTimeMapping {
        RealTimeMapping {
            core: self.core.clone(),
            is_active: self.is_active(),
            target_type: self.unresolved_target.as_ref().map(|t| match t {
                UnresolvedCompoundMappingTarget::Reaper(_) => UnresolvedTargetType::Reaper,
                UnresolvedCompoundMappingTarget::Virtual(_) => UnresolvedTargetType::Virtual,
            }),
            lifecycle_midi_data: self
                .extension
                .lifecycle_midi_data
                .take()
                .unwrap_or_default(),
        }
    }

    pub fn has_virtual_target(&self) -> bool {
        matches!(self.target(), Some(CompoundMappingTarget::Virtual(_)))
    }

    /// Returns `Some` if this affects the mapping's activation state in any way.
    pub fn check_activation_effect(
        &self,
        params: &ParameterArray,
        index: u32,
        previous_value: f32,
    ) -> Option<MappingActivationEffect> {
        let effect_1 =
            self.activation_condition_1
                .is_fulfilled_single(params, index, previous_value);
        let effect_2 =
            self.activation_condition_2
                .is_fulfilled_single(params, index, previous_value);
        MappingActivationEffect::new(self.id(), effect_1, effect_2)
    }

    /// Returns if this target is dynamic.
    pub fn target_can_be_affected_by_parameters(&self) -> bool {
        match &self.unresolved_target {
            Some(UnresolvedCompoundMappingTarget::Reaper(t)) => t.can_be_affected_by_parameters(),
            _ => false,
        }
    }

    /// Returns if this activation condition is affected by parameter changes in general.
    pub fn activation_can_be_affected_by_parameters(&self) -> bool {
        self.activation_condition_1.can_be_affected_by_parameters()
            || self.activation_condition_2.can_be_affected_by_parameters()
    }

    pub fn update_activation_from_effect(
        &mut self,
        activation_effect: MappingActivationEffect,
    ) -> Option<ActivationChange> {
        let was_active_before = self.is_active();
        self.is_active_1 = activation_effect
            .active_1_effect
            .unwrap_or(self.is_active_1);
        self.is_active_2 = activation_effect
            .active_2_effect
            .unwrap_or(self.is_active_2);
        let now_is_active = self.is_active();
        if now_is_active == was_active_before {
            return None;
        }
        let update = ActivationChange {
            id: self.id(),
            is_active: now_is_active,
        };
        Some(update)
    }

    pub fn refresh_all(&mut self, context: ExtendedProcessorContext, params: &ParameterArray) {
        self.refresh_target(context);
        self.update_activation(params);
    }

    pub fn needs_refresh_when_target_touched(&self) -> bool {
        matches!(
            self.unresolved_target,
            Some(UnresolvedCompoundMappingTarget::Reaper(
                UnresolvedReaperTarget::LastTouched
            ))
        )
    }

    pub fn play_pos_feedback_resolution(&self) -> Option<PlayPosFeedbackResolution> {
        let t = self.unresolved_target.as_ref()?;
        t.play_pos_feedback_resolution()
    }

    pub fn wants_to_be_polled_for_control(&self) -> bool {
        self.core.mode.wants_to_be_polled()
    }

    /// The boolean tells if the resolved target changed in some way, the activation change says if
    /// activation changed from off to on or on to off.
    pub fn refresh_target(
        &mut self,
        context: ExtendedProcessorContext,
    ) -> (bool, Option<ActivationChange>) {
        let was_active_before = self.core.options.target_is_active;
        let (target, is_active) = match self.unresolved_target.as_ref() {
            None => (None, false),
            Some(t) => match t.resolve(context).ok() {
                None => (None, false),
                Some(rt) => {
                    let met = t.conditions_are_met(&rt);
                    (Some(rt), met)
                }
            },
        };
        let target_changed = target != self.core.target;
        self.core.target = target;
        self.core.options.target_is_active = is_active;
        if is_active == was_active_before {
            return (target_changed, None);
        }
        let update = ActivationChange {
            id: self.id(),
            is_active,
        };
        (target_changed, Some(update))
    }

    pub fn update_activation(&mut self, params: &ParameterArray) -> Option<ActivationChange> {
        let was_active_before = self.is_active();
        self.is_active_1 = self.activation_condition_1.is_fulfilled(params);
        self.is_active_2 = self.activation_condition_2.is_fulfilled(params);
        let now_is_active = self.is_active();
        if now_is_active == was_active_before {
            return None;
        }
        let update = ActivationChange {
            id: self.id(),
            is_active: now_is_active,
        };
        Some(update)
    }

    pub fn is_active(&self) -> bool {
        self.is_active_1 && self.is_active_2
    }

    fn is_effectively_active(&self) -> bool {
        self.is_active() && self.core.options.target_is_active
    }

    pub fn is_effectively_on(&self) -> bool {
        self.is_effectively_active()
            && (self.core.options.control_is_enabled || self.core.options.feedback_is_enabled)
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.core.options.control_is_enabled
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.core.options.feedback_is_enabled
    }

    pub fn source(&self) -> &CompoundMappingSource {
        &self.core.source
    }

    pub fn target(&self) -> Option<&CompoundMappingTarget> {
        self.core.target.as_ref()
    }

    /// This is for timer-triggered control and works like `control_if_enabled`.
    pub fn poll_if_control_enabled(&mut self) -> Option<FeedbackValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        let final_value = self.core.mode.poll(target)?;
        // Echo feedback, send feedback after control ... all of that is not important when
        // firing triggered by a timer.
        // Be graceful here.
        // TODO-medium In future we could display some kind of small unintrusive error message.
        let _ = target.control(final_value);
        self.feedback_after_control_if_unsupported_by_target(target)
    }

    /// Controls mode => target.
    ///
    /// Don't execute in real-time processor because this executes REAPER main-thread-only
    /// functions. If `send_feedback_after_control` is on, this might return feedback.
    pub fn control_if_enabled(
        &mut self,
        value: ControlValue,
        options: ControlOptions,
    ) -> Option<FeedbackValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        let final_value = self.core.mode.control(value, target);
        if let Some(v) = final_value {
            if self.core.options.prevent_echo_feedback {
                self.core.time_of_last_control = Some(Instant::now());
            }
            // Be graceful here.
            // TODO-medium In future we could display some kind of small unintrusive error message.
            let _ = target.control(v);
            self.feedback_after_control_if_unsupported_by_target(target)
        } else {
            // The target value was not changed. If `send_feedback_after_control` is enabled, we
            // still send feedback - this can be useful with controllers which insist controlling
            // the LED on their own. The feedback sent by ReaLearn will fix this self-controlled
            // LED state.
            self.feedback_after_control_if_enabled(options)
        }
    }

    /// Not usable for mappings with virtual targets.
    fn feedback_after_control_if_unsupported_by_target(
        &self,
        target: &ReaperTarget,
    ) -> Option<FeedbackValue> {
        if target.supports_feedback() {
            // The target value was changed and that triggered feedback. Therefore we don't
            // need to send it here a second time (even if `send_feedback_after_control` is
            // enabled). This happens in the majority of cases.
            None
        } else {
            // The target value was changed but the target doesn't support feedback. If
            // `send_feedback_after_control` is enabled, we at least send feedback after we
            // know it has been changed. What a virtual control mapping says shouldn't be relevant
            // here because this is about the target supporting feedback, not about the controller
            // needing the "Send feedback after control" workaround. Therefore we don't forward
            // any "enforce" options.
            // TODO-low Wouldn't it be better to always send feedback in this situation? But that
            //  could the user let believe that it actually works while in reality it's not "true"
            //  feedback that is independent from control. So an opt-in is maybe the right thing.
            if self.core.options.send_feedback_after_control {
                self.feedback_if_enabled()
            } else {
                None
            }
        }
    }

    pub fn virtual_source_control_element(&self) -> Option<VirtualControlElement> {
        match &self.core.source {
            CompoundMappingSource::Virtual(s) => Some(s.control_element()),
            _ => None,
        }
    }

    pub fn virtual_target_control_element(&self) -> Option<VirtualControlElement> {
        match self.unresolved_target.as_ref()? {
            UnresolvedCompoundMappingTarget::Virtual(t) => Some(t.control_element()),
            _ => None,
        }
    }

    /// Returns `None` when used on mappings with virtual targets.
    pub fn feedback_if_enabled(&self) -> Option<FeedbackValue> {
        if !self.feedback_is_effectively_on() {
            return None;
        }
        self.feedback(true)
    }

    /// Returns `None` when used on mappings with virtual targets.
    pub fn feedback(&self, with_projection_feedback: bool) -> Option<FeedbackValue> {
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        let target_value = target.current_value()?;
        self.feedback_given_target_value(
            target_value,
            with_projection_feedback,
            !self.core.is_echo(),
        )
    }

    pub fn is_echo(&self) -> bool {
        self.core.is_echo()
    }

    pub fn given_or_current_value(
        &self,
        target_value: Option<UnitValue>,
        target: &ReaperTarget,
    ) -> Option<UnitValue> {
        target_value.or_else(|| target.current_value())
    }

    pub fn feedback_given_target_value(
        &self,
        target_value: UnitValue,
        with_projection_feedback: bool,
        with_source_feedback: bool,
    ) -> Option<FeedbackValue> {
        let mode_value = self.core.mode.feedback(target_value)?;
        self.feedback_given_mode_value(mode_value, with_projection_feedback, with_source_feedback)
    }

    pub fn feedback_given_mode_value(
        &self,
        mode_value: UnitValue,
        with_projection_feedback: bool,
        with_source_feedback: bool,
    ) -> Option<FeedbackValue> {
        FeedbackValue::from_mode_value(
            self.core.compartment,
            self.id(),
            &self.core.source,
            mode_value,
            with_projection_feedback,
            with_source_feedback,
        )
    }

    pub fn zero_feedback(&self) -> Option<FeedbackValue> {
        // TODO-medium  "Unused" and "zero" could be a difference for projection so we should
        //  have different values for that (at the moment it's not though).
        self.feedback_given_mode_value(UnitValue::MIN, true, true)
    }

    fn feedback_after_control_if_enabled(&self, options: ControlOptions) -> Option<FeedbackValue> {
        if self.core.options.send_feedback_after_control
            || options.enforce_send_feedback_after_control
        {
            if self.feedback_is_effectively_on() {
                // No projection feedback in this case! Just the source controller needs this hack.
                self.feedback(false)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn control_osc_virtualizing(&mut self, msg: &OscMessage) -> Option<PartialControlMatch> {
        self.core.target.as_ref()?;
        let control_value = if let CompoundMappingSource::Osc(s) = &self.core.source {
            s.control(msg)?
        } else {
            return None;
        };
        match_partially(&mut self.core, control_value)
    }
}

#[derive(Debug)]
pub struct RealTimeMapping {
    core: MappingCore,
    is_active: bool,
    target_type: Option<UnresolvedTargetType>,
    lifecycle_midi_data: LifecycleMidiData,
}

#[derive(Debug)]
pub enum UnresolvedTargetType {
    Reaper,
    Virtual,
}

#[derive(Copy, Clone, Debug)]
pub enum LifecyclePhase {
    Activation,
    Deactivation,
}

impl From<bool> for LifecyclePhase {
    fn from(v: bool) -> Self {
        use LifecyclePhase::*;
        if v { Activation } else { Deactivation }
    }
}

impl RealTimeMapping {
    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn lifecycle_midi_messages(&self, phase: LifecyclePhase) -> &[LifecycleMidiMessage] {
        use LifecyclePhase::*;
        match phase {
            Activation => &self.lifecycle_midi_data.activation_midi_messages,
            Deactivation => &self.lifecycle_midi_data.deactivation_midi_messages,
        }
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.core.options.control_is_enabled
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.core.options.feedback_is_enabled
    }

    pub fn feedback_is_effectively_on_ignoring_mapping_activation(&self) -> bool {
        self.is_effectively_active_ignoring_mapping_activation()
            && self.core.options.feedback_is_enabled
    }

    pub fn feedback_is_effectively_on_ignoring_target_activation(&self) -> bool {
        self.is_effectively_active_ignoring_target_activation()
            && self.core.options.feedback_is_enabled
    }

    fn is_effectively_active(&self) -> bool {
        self.is_active && self.core.options.target_is_active
    }

    fn is_effectively_active_ignoring_target_activation(&self) -> bool {
        self.is_active
    }

    fn is_effectively_active_ignoring_mapping_activation(&self) -> bool {
        self.core.options.target_is_active
    }

    pub fn update_target_activation(&mut self, is_active: bool) {
        self.core.options.target_is_active = is_active;
    }

    pub fn update_activation(&mut self, is_active: bool) {
        self.is_active = is_active
    }

    pub fn source(&self) -> &CompoundMappingSource {
        &self.core.source
    }

    pub fn zero_feedback_midi_source_value(&self) -> Option<MidiSourceValue<RawShortMessage>> {
        if let CompoundMappingSource::Midi(source) = &self.core.source {
            source.feedback(UnitValue::MIN)
        } else {
            None
        }
    }

    pub fn has_virtual_target(&self) -> bool {
        matches!(self.target_type, Some(UnresolvedTargetType::Virtual))
    }

    pub fn has_reaper_target(&self) -> bool {
        matches!(self.target_type, Some(UnresolvedTargetType::Reaper))
    }

    pub fn consumes(&self, msg: RawShortMessage) -> bool {
        self.core.source.consumes(&msg)
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn control_midi_virtualizing(
        &mut self,
        source_value: &MidiSourceValue<RawShortMessage>,
    ) -> Option<PartialControlMatch> {
        self.core.target.as_ref()?;
        let control_value = if let CompoundMappingSource::Midi(s) = &self.core.source {
            s.control(&source_value)?
        } else {
            return None;
        };
        match_partially(&mut self.core, control_value)
    }
}

pub enum PartialControlMatch {
    ProcessVirtual(VirtualSourceValue),
    ProcessDirect(ControlValue),
}

#[derive(Clone, Debug)]
pub struct MappingCore {
    compartment: MappingCompartment,
    id: MappingId,
    source: CompoundMappingSource,
    mode: Mode,
    target: Option<CompoundMappingTarget>,
    options: ProcessorMappingOptions,
    time_of_last_control: Option<Instant>,
}

impl MappingCore {
    fn is_echo(&self) -> bool {
        if let Some(t) = self.time_of_last_control {
            t.elapsed() <= MAX_ECHO_FEEDBACK_DELAY
        } else {
            false
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub enum CompoundMappingSource {
    Midi(MidiSource),
    Osc(OscSource),
    Virtual(VirtualSource),
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub struct QualifiedSource {
    pub compartment: MappingCompartment,
    pub id: MappingId,
    pub source: CompoundMappingSource,
}

impl QualifiedSource {
    pub fn zero_feedback(&self) -> Option<FeedbackValue> {
        FeedbackValue::from_mode_value(
            self.compartment,
            self.id,
            &self.source,
            UnitValue::MIN,
            true,
            true,
        )
    }
}

impl CompoundMappingSource {
    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.format_control_value(value),
            Virtual(s) => s.format_control_value(value),
            Osc(s) => s.format_control_value(value),
        }
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.parse_control_value(text),
            Virtual(s) => s.parse_control_value(text),
            Osc(s) => s.parse_control_value(text),
        }
    }

    pub fn character(&self) -> ExtendedSourceCharacter {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => ExtendedSourceCharacter::Normal(s.character()),
            Virtual(s) => s.character(),
            Osc(s) => ExtendedSourceCharacter::Normal(s.character()),
        }
    }

    pub fn feedback(&self, feedback_value: UnitValue) -> Option<SourceFeedbackValue> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.feedback(feedback_value).map(SourceFeedbackValue::Midi),
            Osc(s) => s.feedback(feedback_value).map(SourceFeedbackValue::Osc),
            // This is handled in a special way by consumers.
            Virtual(_) => None,
        }
    }

    pub fn consumes(&self, msg: &impl ShortMessage) -> bool {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.consumes(msg),
            Virtual(_) | Osc(_) => false,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum FeedbackValue {
    Virtual {
        with_projection_feedback: bool,
        with_source_feedback: bool,
        value: VirtualSourceValue,
    },
    Real(RealFeedbackValue),
}

impl FeedbackValue {
    pub fn from_mode_value(
        compartment: MappingCompartment,
        id: MappingId,
        source: &CompoundMappingSource,
        mode_value: UnitValue,
        with_projection_feedback: bool,
        with_source_feedback: bool,
    ) -> Option<FeedbackValue> {
        if !with_projection_feedback && !with_source_feedback {
            return None;
        }
        let val = if let CompoundMappingSource::Virtual(vs) = &source {
            FeedbackValue::Virtual {
                with_projection_feedback,
                with_source_feedback,
                value: vs.feedback(mode_value),
            }
        } else {
            let projection = if with_projection_feedback
                && compartment == MappingCompartment::ControllerMappings
            {
                Some(ProjectionFeedbackValue::new(id, mode_value))
            } else {
                None
            };
            let source = if with_source_feedback {
                source.feedback(mode_value)
            } else {
                None
            };
            FeedbackValue::Real(RealFeedbackValue::new(projection, source)?)
        };
        Some(val)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct RealFeedbackValue {
    /// Feedback to be sent to projection.
    ///
    /// This is an option because there are situations when we don't want projection feedback but
    /// source feedback (e.g. for "Feedback after control" because of too clever controllers).
    pub projection: Option<ProjectionFeedbackValue>,
    /// Feedback to be sent to the source.
    ///
    /// This is an option because there are situations when we don't want source feedback but
    /// projection feedback (e.g. if "MIDI feedback output" is set to None).
    pub source: Option<SourceFeedbackValue>,
}

impl RealFeedbackValue {
    pub fn new(
        projection: Option<ProjectionFeedbackValue>,
        source: Option<SourceFeedbackValue>,
    ) -> Option<Self> {
        if projection.is_none() && source.is_none() {
            return None;
        }
        let val = Self { projection, source };
        Some(val)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ProjectionFeedbackValue {
    pub mapping_id: MappingId,
    pub value: UnitValue,
}

impl ProjectionFeedbackValue {
    pub fn new(mapping_id: MappingId, value: UnitValue) -> Self {
        Self { mapping_id, value }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum SourceFeedbackValue {
    Midi(MidiSourceValue<RawShortMessage>),
    Osc(OscMessage),
}

#[derive(Debug)]
pub enum UnresolvedCompoundMappingTarget {
    Reaper(UnresolvedReaperTarget),
    Virtual(VirtualTarget),
}

impl UnresolvedCompoundMappingTarget {
    pub fn resolve(
        &self,
        context: ExtendedProcessorContext,
    ) -> Result<CompoundMappingTarget, &'static str> {
        use UnresolvedCompoundMappingTarget::*;
        let resolved = match self {
            Reaper(t) => CompoundMappingTarget::Reaper(t.resolve(context)?),
            Virtual(t) => CompoundMappingTarget::Virtual(*t),
        };
        Ok(resolved)
    }

    pub fn conditions_are_met(&self, target: &CompoundMappingTarget) -> bool {
        use UnresolvedCompoundMappingTarget::*;
        match (self, target) {
            (Reaper(t), CompoundMappingTarget::Reaper(rt)) => t.conditions_are_met(rt),
            (Virtual(_), CompoundMappingTarget::Virtual(_)) => true,
            _ => unreachable!(),
        }
    }

    pub fn play_pos_feedback_resolution(&self) -> Option<PlayPosFeedbackResolution> {
        use UnresolvedCompoundMappingTarget::*;
        match self {
            Reaper(t) => t.play_pos_feedback_resolution(),
            Virtual(_) => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum CompoundMappingTarget {
    Reaper(ReaperTarget),
    Virtual(VirtualTarget),
}

impl RealearnTarget for CompoundMappingTarget {
    fn character(&self) -> TargetCharacter {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.character(),
            Virtual(t) => t.character(),
        }
    }

    fn open(&self) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.open(),
            Virtual(_) => {}
        };
    }
    fn parse_as_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.parse_as_value(text),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(&self, text: &str) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.parse_as_step_size(text),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn convert_unit_value_to_discrete_value(&self, input: UnitValue) -> Result<u32, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.convert_unit_value_to_discrete_value(input),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn format_value_without_unit(&self, value: UnitValue) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_value_without_unit(value),
            Virtual(_) => String::new(),
        }
    }

    fn format_step_size_without_unit(&self, step_size: UnitValue) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_step_size_without_unit(step_size),
            Virtual(_) => String::new(),
        }
    }

    fn hide_formatted_value(&self) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.hide_formatted_value(),
            Virtual(_) => false,
        }
    }

    fn hide_formatted_step_size(&self) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.hide_formatted_step_size(),
            Virtual(_) => false,
        }
    }

    fn value_unit(&self) -> &'static str {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.value_unit(),
            Virtual(_) => "",
        }
    }

    fn step_size_unit(&self) -> &'static str {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.step_size_unit(),
            Virtual(_) => "",
        }
    }

    fn format_value(&self, value: UnitValue) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_value(value),
            Virtual(_) => String::new(),
        }
    }

    fn control(&self, value: ControlValue) -> Result<(), &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control(value),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn can_report_current_value(&self) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.can_report_current_value(),
            Virtual(_) => false,
        }
    }
}

impl Target for CompoundMappingTarget {
    fn current_value(&self) -> Option<UnitValue> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.current_value(),
            Virtual(t) => t.current_value(),
        }
    }

    fn control_type(&self) -> ControlType {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control_type(),
            Virtual(t) => t.control_type(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct QualifiedMappingId {
    pub compartment: MappingCompartment,
    pub id: MappingId,
}

impl QualifiedMappingId {
    pub fn new(compartment: MappingCompartment, id: MappingId) -> Self {
        Self { compartment, id }
    }
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Debug,
    Enum,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum MappingCompartment {
    // It's important for `RealTimeProcessor` logic that this is the first element! We use array
    // destructuring.
    #[display(fmt = "Controller mappings")]
    ControllerMappings,
    #[display(fmt = "Main mappings")]
    MainMappings,
}

impl MappingCompartment {
    /// We could also use the generated `into_enum_iter()` everywhere but IDE completion
    /// in IntelliJ Rust doesn't work for that at the time of this writing.
    pub fn enum_iter() -> impl Iterator<Item = MappingCompartment> + ExactSizeIterator {
        MappingCompartment::into_enum_iter()
    }
}

pub enum ExtendedSourceCharacter {
    Normal(SourceCharacter),
    VirtualContinuous,
}

fn match_partially(
    core: &mut MappingCore,
    control_value: ControlValue,
) -> Option<PartialControlMatch> {
    use CompoundMappingTarget::*;
    let result = match core.target.as_ref()? {
        Reaper(_) => {
            // Send to main processor because this needs to be done in main thread.
            PartialControlMatch::ProcessDirect(control_value)
        }
        Virtual(t) => {
            // Determine resulting virtual control value in real-time processor.
            // It's important to do that here. We need to know the result in order to
            // return if there was actually a match of *real* non-virtual mappings.
            // Unlike with REAPER targets, we also don't have threading issues here :)
            // TODO-high Mode polling in real-time processor, too!
            let transformed_control_value = core.mode.control(control_value, t)?;
            if core.options.prevent_echo_feedback {
                core.time_of_last_control = Some(Instant::now());
            }
            PartialControlMatch::ProcessVirtual(VirtualSourceValue::new(
                t.control_element(),
                transformed_control_value,
            ))
        }
    };
    Some(result)
}

#[derive(PartialEq, Debug)]
pub(crate) enum ControlMode {
    Disabled,
    Controlling,
    LearningSource {
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    },
}
