use crate::domain::{
    ActivationChange, ActivationCondition, AdditionalFeedbackEvent, ControlContext, ControlOptions,
    ExtendedProcessorContext, InstanceFeedbackEvent, MappingActivationEffect, MidiSource, Mode,
    ParameterArray, ParameterSlice, PlayPosFeedbackResolution, RealSource, RealTimeReaperTarget,
    RealearnTarget, ReaperTarget, TargetCharacter, TrackExclusivity, UnresolvedReaperTarget,
    VirtualControlElement, VirtualSource, VirtualSourceValue, VirtualTarget,
    COMPARTMENT_PARAMETER_COUNT,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use helgoboss_learn::{
    ControlType, ControlValue, MidiSourceValue, ModeControlOptions, OscSource, RawMidiEvent,
    SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{RawShortMessage, ShortMessage};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use reaper_high::{ChangeEvent, Fx, Project, Track, TrackRoute};
use rosc::OscMessage;
use serde::{Deserialize, Serialize};
use smallvec::alloc::fmt::Formatter;
use std::fmt;
use std::fmt::Display;
use std::ops::Range;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct ProcessorMappingOptions {
    /// In the main processor mapping this might be overridden by the unresolved target's
    /// is_always_active() result. The real-time processor always gets the effective result of the
    /// main processor mapping.
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
    /// Is `Some` if the user-provided target data is complete.
    unresolved_target: Option<UnresolvedCompoundMappingTarget>,
    /// Is non-empty if the target resolved successfully.
    targets: Vec<CompoundMappingTarget>,
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
                options,
                time_of_last_control: None,
            },
            unresolved_target,
            targets: vec![],
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

    pub fn mode_control_options(&self) -> ModeControlOptions {
        self.core.mode_control_options()
    }

    pub fn splinter_real_time_mapping(&mut self) -> RealTimeMapping {
        RealTimeMapping {
            core: MappingCore {
                options: ProcessorMappingOptions {
                    target_is_active: self.target_is_effectively_active(),
                    ..self.core.options
                },
                ..self.core.clone()
            },
            is_active: self.is_active(),
            target_category: self.unresolved_target.as_ref().map(|t| match t {
                UnresolvedCompoundMappingTarget::Reaper(_) => UnresolvedTargetCategory::Reaper,
                UnresolvedCompoundMappingTarget::Virtual(_) => UnresolvedTargetCategory::Virtual,
            }),
            target_is_resolved: !self.targets.is_empty(),
            resolved_target: self
                .targets
                .first()
                .and_then(|t| t.splinter_real_time_target()),
            lifecycle_midi_data: self
                .extension
                .lifecycle_midi_data
                .take()
                .unwrap_or_default(),
        }
    }

    pub fn has_virtual_target(&self) -> bool {
        self.virtual_target().is_some()
    }

    pub fn virtual_target(&self) -> Option<&VirtualTarget> {
        if let Some(UnresolvedCompoundMappingTarget::Virtual(t)) = self.unresolved_target.as_ref() {
            Some(t)
        } else {
            None
        }
    }

    pub fn has_reaper_target(&self) -> bool {
        self.unresolved_reaper_target().is_some()
    }

    pub fn has_resolved_successfully(&self) -> bool {
        !self.targets.is_empty()
    }

    pub fn unresolved_reaper_target(&self) -> Option<&UnresolvedReaperTarget> {
        if let Some(UnresolvedCompoundMappingTarget::Reaper(t)) = self.unresolved_target.as_ref() {
            Some(t)
        } else {
            None
        }
    }

    /// Returns `Some` if this affects the mapping's activation state in any way.
    pub fn check_activation_effect(
        &self,
        params: &ParameterArray,
        absolute_param_index: u32,
        previous_value: f32,
    ) -> Option<MappingActivationEffect> {
        let sliced_params = self.core.compartment.slice_params(params);
        let rel_param_index = self
            .core
            .compartment
            .relativize_absolute_index(absolute_param_index);
        let effect_1 = self.activation_condition_1.is_fulfilled_single(
            sliced_params,
            rel_param_index,
            previous_value,
        );
        let effect_2 = self.activation_condition_2.is_fulfilled_single(
            sliced_params,
            rel_param_index,
            previous_value,
        );
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

    pub fn refresh_all(&mut self, context: ExtendedProcessorContext) {
        self.refresh_target(context);
        self.update_activation(&context.params());
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
        let was_effectively_active_before = self.target_is_effectively_active();
        let (targets, is_active) = match self.unresolved_target.as_ref() {
            None => (vec![], false),
            Some(t) => match t.resolve(context, self.core.compartment).ok() {
                None => (vec![], false),
                Some(resolved_targets) => {
                    let met = t.conditions_are_met(&resolved_targets);
                    (resolved_targets, met)
                }
            },
        };
        let target_changed = targets != self.targets;
        self.targets = targets;
        self.core.options.target_is_active = is_active;
        if self.target_is_effectively_active() == was_effectively_active_before {
            return (target_changed, None);
        }
        let update = ActivationChange {
            id: self.id(),
            is_active,
        };
        (target_changed, Some(update))
    }

    pub fn update_activation(&mut self, params: &ParameterArray) -> Option<ActivationChange> {
        let sliced_params = self.core.compartment.slice_params(params);
        let was_active_before = self.is_active();
        self.is_active_1 = self.activation_condition_1.is_fulfilled(sliced_params);
        self.is_active_2 = self.activation_condition_2.is_fulfilled(sliced_params);
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
        self.is_active() && self.target_is_effectively_active()
    }

    fn target_is_effectively_active(&self) -> bool {
        if self.core.options.target_is_active {
            return true;
        }
        if let Some(t) = self.unresolved_reaper_target() {
            t.is_always_active()
        } else {
            false
        }
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

    pub fn has_this_real_source(&self, source: &RealSource) -> bool {
        match &self.core.source {
            CompoundMappingSource::Midi(self_source) => {
                matches!(source, RealSource::Midi(s) if s == self_source)
            }
            CompoundMappingSource::Osc(self_source) => {
                matches!(source, RealSource::Osc(s) if s == self_source)
            }
            CompoundMappingSource::Virtual(_) => false,
        }
    }

    pub fn targets(&self) -> &[CompoundMappingTarget] {
        &self.targets
    }

    /// This is for timer-triggered control and works like `control_if_enabled`.
    pub fn poll_if_control_enabled(&mut self, context: ControlContext) -> Option<FeedbackValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let mut should_send_feedback = false;
        for target in &self.targets {
            let target = if let CompoundMappingTarget::Reaper(t) = target {
                t
            } else {
                continue;
            };
            let final_value = if let Some(v) = self.core.mode.poll(target, Some(context)) {
                v
            } else {
                continue;
            };
            // Echo feedback, send feedback after control ... all of that is not important when
            // firing triggered by a timer.
            // Be graceful here. Don't debug-log errors for now because this is polled.
            let _ = target.control(final_value, context);
            if self.should_send_non_auto_feedback_after_control(target) {
                should_send_feedback = true;
            }
        }
        if should_send_feedback {
            self.feedback(true, context)
        } else {
            None
        }
    }

    /// Controls mode => target.
    ///
    /// Don't execute in real-time processor because this executes REAPER main-thread-only
    /// functions. If `send_feedback_after_control` is on, this might return feedback.
    pub fn control_if_enabled(
        &mut self,
        value: ControlValue,
        options: ControlOptions,
        context: ControlContext,
        logger: &slog::Logger,
    ) -> Option<FeedbackValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let mut send_feedback = false;
        let mut at_least_one_target_val_was_changed = false;
        for target in &self.targets {
            let target = if let CompoundMappingTarget::Reaper(t) = target {
                t
            } else {
                continue;
            };
            let final_value = self.core.mode.control_with_options(
                value,
                target,
                Some(context),
                options.mode_control_options,
            );
            if let Some(v) = final_value {
                at_least_one_target_val_was_changed = true;
                // Be graceful here.
                if let Err(msg) = target.control(v, context) {
                    slog::debug!(logger, "Control failed: {}", msg);
                }
                if self.should_send_non_auto_feedback_after_control(target) {
                    send_feedback = true;
                }
            } else {
                // The target value was not changed. If `send_feedback_after_control` is enabled, we
                // still send feedback - this can be useful with controllers which insist
                // controlling the LED on their own. The feedback sent by ReaLearn
                // will fix this self-controlled LED state.
                send_feedback = true;
            }
        }
        if at_least_one_target_val_was_changed {
            if self.core.options.prevent_echo_feedback {
                self.core.time_of_last_control = Some(Instant::now());
            }
            if send_feedback {
                self.feedback(true, context)
            } else {
                None
            }
        } else if send_feedback {
            self.feedback_after_control_if_enabled(options, context)
        } else {
            None
        }
    }

    /// Not usable for mappings with virtual targets.
    fn should_send_non_auto_feedback_after_control(&self, target: &ReaperTarget) -> bool {
        if target.supports_automatic_feedback() {
            // The target value was changed and that triggered feedback. Therefore we don't
            // need to send it here a second time (even if `send_feedback_after_control` is
            // enabled). This happens in the majority of cases.
            false
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
            self.core.options.send_feedback_after_control && self.feedback_is_effectively_on()
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
    pub fn feedback(
        &self,
        with_projection_feedback: bool,
        context: ControlContext,
    ) -> Option<FeedbackValue> {
        let combined_target_value = self
            .targets
            .iter()
            .filter_map(|target| match target {
                CompoundMappingTarget::Reaper(t) => t.current_value(Some(context)),
                _ => None,
            })
            .max()?;
        self.feedback_given_target_value(
            combined_target_value,
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
        context: ControlContext,
    ) -> Option<UnitValue> {
        target_value.or_else(|| target.current_value(Some(context)))
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

    fn feedback_after_control_if_enabled(
        &self,
        options: ControlOptions,
        context: ControlContext,
    ) -> Option<FeedbackValue> {
        if self.core.options.send_feedback_after_control
            || options.enforce_send_feedback_after_control
        {
            if self.feedback_is_effectively_on() {
                // No projection feedback in this case! Just the source controller needs this hack.
                self.feedback(false, context)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn control_osc_virtualizing(&mut self, msg: &OscMessage) -> Option<PartialControlMatch> {
        if self.targets.is_empty() {
            return None;
        }
        let control_value = if let CompoundMappingSource::Osc(s) = &self.core.source {
            s.control(msg)?
        } else {
            return None;
        };
        // First target is enough because this does nothing yet.
        match self.targets.first()? {
            CompoundMappingTarget::Reaper(_) => {
                Some(PartialControlMatch::ProcessDirect(control_value))
            }
            CompoundMappingTarget::Virtual(t) => match_partially(&mut self.core, t, control_value),
        }
    }
}

#[derive(Debug)]
pub struct RealTimeMapping {
    pub core: MappingCore,
    is_active: bool,
    /// Is `Some` if user-provided target data is complete.
    target_category: Option<UnresolvedTargetCategory>,
    target_is_resolved: bool,
    /// Is `Some` if this target needs to be processed in real-time.
    pub resolved_target: Option<RealTimeCompoundMappingTarget>,
    pub lifecycle_midi_data: LifecycleMidiData,
}

#[derive(Debug)]
pub enum UnresolvedTargetCategory {
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

    pub fn has_reaper_target(&self) -> bool {
        matches!(self.target_category, Some(UnresolvedTargetCategory::Reaper))
    }

    pub fn consumes(&self, msg: RawShortMessage) -> bool {
        self.core.source.consumes(&msg)
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn mode_control_options(&self) -> ModeControlOptions {
        self.core.mode_control_options()
    }

    pub fn control_midi_virtualizing(
        &mut self,
        source_value: &MidiSourceValue<RawShortMessage>,
    ) -> Option<PartialControlMatch> {
        if !self.target_is_resolved {
            return None;
        }
        let control_value = if let CompoundMappingSource::Midi(s) = &self.core.source {
            s.control(&source_value)?
        } else {
            return None;
        };
        match self.resolved_target.as_ref()? {
            RealTimeCompoundMappingTarget::Reaper(_) => {
                Some(PartialControlMatch::ProcessDirect(control_value))
            }
            RealTimeCompoundMappingTarget::Virtual(t) => {
                match_partially(&mut self.core, t, control_value)
            }
        }
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
    pub source: CompoundMappingSource,
    pub mode: Mode,
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

    fn mode_control_options(&self) -> ModeControlOptions {
        ModeControlOptions {
            enforce_rotate: self.mode.rotate,
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
        compartment: MappingCompartment,
    ) -> Result<Vec<CompoundMappingTarget>, &'static str> {
        use UnresolvedCompoundMappingTarget::*;
        let resolved_targets = match self {
            Reaper(t) => {
                let reaper_targets = t.resolve(context, compartment)?;
                reaper_targets
                    .into_iter()
                    .map(CompoundMappingTarget::Reaper)
                    .collect()
            }
            Virtual(t) => vec![CompoundMappingTarget::Virtual(*t)],
        };
        Ok(resolved_targets)
    }

    pub fn conditions_are_met(&self, targets: &[CompoundMappingTarget]) -> bool {
        use UnresolvedCompoundMappingTarget::*;
        targets.iter().all(|target| match (self, target) {
            (Reaper(t), CompoundMappingTarget::Reaper(rt)) => t.conditions_are_met(rt),
            (Virtual(_), CompoundMappingTarget::Virtual(_)) => true,
            _ => unreachable!(),
        })
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

impl CompoundMappingTarget {
    pub fn splinter_real_time_target(&self) -> Option<RealTimeCompoundMappingTarget> {
        match self {
            CompoundMappingTarget::Reaper(t) => t
                .splinter_real_time_target()
                .map(RealTimeCompoundMappingTarget::Reaper),
            CompoundMappingTarget::Virtual(t) => Some(RealTimeCompoundMappingTarget::Virtual(*t)),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum RealTimeCompoundMappingTarget {
    Reaper(RealTimeReaperTarget),
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

    fn control_type_and_character(&self) -> (ControlType, TargetCharacter) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control_type_and_character(),
            Virtual(t) => (t.control_type(), t.character()),
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

    fn control(&self, value: ControlValue, context: ControlContext) -> Result<(), &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control(value, context),
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

    fn is_available(&self) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.is_available(),
            Virtual(_) => true,
        }
    }

    fn project(&self) -> Option<Project> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.project(),
            Virtual(_) => None,
        }
    }

    fn track(&self) -> Option<&Track> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.track(),
            Virtual(_) => None,
        }
    }

    fn fx(&self) -> Option<&Fx> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.fx(),
            Virtual(_) => None,
        }
    }

    fn route(&self) -> Option<&TrackRoute> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.route(),
            Virtual(_) => None,
        }
    }

    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.track_exclusivity(),
            Virtual(_) => None,
        }
    }

    fn supports_automatic_feedback(&self) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.supports_automatic_feedback(),
            Virtual(_) => false,
        }
    }

    fn process_change_event(
        &self,
        evt: &ChangeEvent,
        control_context: ControlContext,
    ) -> (bool, Option<UnitValue>) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.process_change_event(evt, control_context),
            Virtual(_) => (false, None),
        }
    }

    fn value_changed_from_additional_feedback_event(
        &self,
        evt: &AdditionalFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.value_changed_from_additional_feedback_event(evt),
            Virtual(_) => (false, None),
        }
    }

    fn value_changed_from_instance_feedback_event(
        &self,
        evt: &InstanceFeedbackEvent,
    ) -> (bool, Option<UnitValue>) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.value_changed_from_instance_feedback_event(evt),
            Virtual(_) => (false, None),
        }
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.splinter_real_time_target(),
            Virtual(_) => None,
        }
    }

    fn convert_discrete_value_to_unit_value(&self, value: u32) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.convert_discrete_value_to_unit_value(value),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }
}

impl<'a> Target<'a> for CompoundMappingTarget {
    type Context = Option<ControlContext<'a>>;

    fn current_value(&self, context: Option<ControlContext>) -> Option<UnitValue> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.current_value(context),
            Virtual(t) => t.current_value(()),
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

    pub fn by_absolute_param_index(absolute_index: u32) -> Option<MappingCompartment> {
        Self::enum_iter().find(|c| c.param_range().contains(&(absolute_index)))
    }

    pub fn relativize_absolute_index(self, absolute_index: u32) -> u32 {
        absolute_index - self.param_offset()
    }

    pub fn slice_params(self, params: &ParameterArray) -> &ParameterSlice {
        let range = self.param_range();
        &params[range.start as usize..range.end as usize]
    }

    const fn param_offset(self) -> u32 {
        match self {
            MappingCompartment::ControllerMappings => 100u32,
            MappingCompartment::MainMappings => 0u32,
        }
    }

    pub const fn param_range(self) -> Range<u32> {
        let offset = self.param_offset();
        offset..(offset + COMPARTMENT_PARAMETER_COUNT)
    }
}

pub enum ExtendedSourceCharacter {
    Normal(SourceCharacter),
    VirtualContinuous,
}

fn match_partially(
    core: &mut MappingCore,
    target: &VirtualTarget,
    control_value: ControlValue,
) -> Option<PartialControlMatch> {
    // Determine resulting virtual control value in real-time processor.
    // It's important to do that here. We need to know the result in order to
    // return if there was actually a match of *real* non-virtual mappings.
    // Unlike with REAPER targets, we also don't have threading issues here :)
    // TODO-medium If we want to support fire after timeout and turbo for mappings with
    //  virtual targets one day, we need to poll this in real-time processor and OSC
    //  processing, too!
    let transformed_control_value = core.mode.control(control_value, target, ())?;
    if core.options.prevent_echo_feedback {
        core.time_of_last_control = Some(Instant::now());
    }
    let res = PartialControlMatch::ProcessVirtual(VirtualSourceValue::new(
        target.control_element(),
        transformed_control_value,
    ));
    Some(res)
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
