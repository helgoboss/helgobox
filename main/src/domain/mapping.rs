use crate::domain::{
    get_prop_value, prop_feedback_resolution, prop_is_affected_by, ActivationChange,
    ActivationCondition, BoxedHitInstruction, CompartmentParamIndex, CompoundChangeEvent,
    ControlContext, ControlEvent, ControlEventTimestamp, ControlOptions, ExtendedProcessorContext,
    FeedbackResolution, GroupId, HitResponse, KeyMessage, KeySource, MappingActivationEffect,
    MappingControlContext, MappingData, MappingInfo, MessageCaptureEvent, MidiScanResult,
    MidiSource, Mode, OscDeviceId, OscScanResult, PersistentMappingProcessingState,
    PluginParamIndex, PluginParams, RealTimeMappingUpdate, RealTimeReaperTarget,
    RealTimeTargetUpdate, RealearnParameterChangePayload, RealearnParameterSource, RealearnTarget,
    ReaperMessage, ReaperSource, ReaperTarget, ReaperTargetType, Tag, TargetCharacter,
    TrackExclusivity, UnresolvedReaperTarget, VirtualControlElement, VirtualFeedbackValue,
    VirtualSource, VirtualSourceAddress, VirtualSourceValue, VirtualTarget,
    COMPARTMENT_PARAMETER_COUNT,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use helgoboss_learn::{
    format_percentage_without_unit, parse_percentage_without_unit, AbsoluteValue, ControlResult,
    ControlType, ControlValue, FeedbackValue, GroupInteraction, MidiSourceAddress, MidiSourceValue,
    ModeControlOptions, ModeControlResult, ModeFeedbackOptions, NumericFeedbackValue, NumericValue,
    OscSource, OscSourceAddress, PreliminaryMidiSourceFeedbackValue, PropValue, RawMidiEvent,
    SourceCharacter, SourceContext, Target, UnitValue, ValueFormatter, ValueParser,
};
use helgoboss_midi::{Channel, RawShortMessage, ShortMessage};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::borrow::Cow;
use std::cell::Cell;

use crate::domain::unresolved_reaper_target::UnresolvedReaperTargetDef;
use indexmap::map::IndexMap;
use indexmap::set::IndexSet;
use reaper_high::{Fx, Project, Track, TrackRoute};
use reaper_medium::MidiInputDeviceId;
use rosc::OscMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct ProcessorMappingOptions {
    /// In the main processor mapping this might be overridden by the unresolved target's
    /// is_always_active() result. The real-time processor always gets the effective result of the
    /// main processor mapping.
    pub target_is_active: bool,
    pub persistent_processing_state: PersistentMappingProcessingState,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub feedback_send_behavior: FeedbackSendBehavior,
    pub beep_on_success: bool,
}

impl ProcessorMappingOptions {
    pub fn control_is_effectively_enabled(&self) -> bool {
        self.persistent_processing_state.is_enabled && self.control_is_enabled
    }

    pub fn feedback_is_effectively_enabled(&self) -> bool {
        self.persistent_processing_state.is_enabled && self.feedback_is_enabled
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
pub enum FeedbackSendBehavior {
    #[display(fmt = "Normal")]
    Normal,
    #[display(fmt = "Send feedback after control")]
    SendFeedbackAfterControl,
    #[display(fmt = "Prevent echo feedback")]
    PreventEchoFeedback,
}

impl Default for FeedbackSendBehavior {
    fn default() -> Self {
        Self::Normal
    }
}

/// Internal technical mapping identifier, not persistent.
///
/// Goals: Quick lookup, guaranteed uniqueness, cheap copy
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MappingId(Uuid);

impl MappingId {
    pub fn random() -> MappingId {
        Self(Uuid::new_v4())
    }
}

impl Default for MappingId {
    fn default() -> Self {
        Self::random()
    }
}

/// A potentially user-defined mapping identifier, persistent
///
/// Goals: For external references (e.g. from API or in projection)
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MappingKey(String);

impl MappingKey {
    pub fn random() -> Self {
        Self(nanoid::nanoid!())
    }
}

impl AsRef<str> for MappingKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for MappingKey {
    fn from(v: String) -> Self {
        Self(v)
    }
}

impl From<MappingKey> for String {
    fn from(v: MappingKey) -> Self {
        v.0
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
    // We need to clone this when producing feedback, pretty often ... so wrapping it in a Rc
    // saves us from doing too much copying and allocation that potentially slows down things
    // (albeit only marginally).
    key: Rc<str>,
    /// This is set only temporarily during mapping sync.
    name: Option<String>,
    tags: Vec<Tag>,
    /// Is `Some` if the user-provided target data is complete.
    unresolved_target: Option<UnresolvedCompoundMappingTarget>,
    /// Is non-empty if the target resolved successfully.
    targets: Vec<CompoundMappingTarget>,
    activation_condition_1: ActivationCondition,
    activation_condition_2: ActivationCondition,
    activation_state: ActivationState,
    extension: MappingExtension,
    initial_target_value: Option<AbsoluteValue>,
    /// Called "y_last" in the control transformation formula.
    last_non_performance_target_value: Cell<Option<AbsoluteValue>>,
}

#[derive(Default, Debug)]
struct ActivationState {
    is_active_1: bool,
    is_active_2: bool,
}

impl ActivationState {
    pub fn is_active(&self) -> bool {
        self.is_active_1 && self.is_active_2
    }
}

impl MainMapping {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        compartment: Compartment,
        id: MappingId,
        key: &MappingKey,
        group_id: GroupId,
        name: String,
        tags: Vec<Tag>,
        source: CompoundMappingSource,
        mode: Mode,
        group_interaction: GroupInteraction,
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
                group_id,
                source,
                mode,
                group_interaction,
                options,
                time_of_last_control: None,
            },
            key: {
                let key_str: &str = key.as_ref();
                key_str.into()
            },
            name: Some(name),
            tags,
            unresolved_target,
            targets: vec![],
            activation_condition_1,
            activation_condition_2,
            activation_state: Default::default(),
            extension,
            initial_target_value: None,
            last_non_performance_target_value: Cell::new(None),
        }
    }

    fn beep_on_success(&self) -> bool {
        self.core.options.beep_on_success
    }

    /// This is for:
    ///
    /// 1. Determining whether to send feedback and optionally, what feedback value to send.
    /// 2. Updating y_last (for performance mappings)
    ///
    /// This method is not required to already return the new target value. If not, the consumer
    /// must query the target for the current value.
    pub fn process_change_event(
        &self,
        target: &ReaperTarget,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        // Textual feedback relates to whatever properties are mentioned in the text expression.
        // But even numeric feedback can use properties - as part of the feedback style
        // (color etc.). That means we need to check for each of these mentioned properties if
        // they might be affected by the incoming event.
        let props_are_affected = self
            .core
            .mode
            .feedback_props_in_use()
            .iter()
            .any(|p| prop_is_affected_by(p, evt, self, target, context));
        let (is_affected, new_value, handle_performance_mapping) =
            if self.core.mode.wants_textual_feedback() {
                // For textual feedback only those props matter. Updating y_last is not relevant because
                // textual feedback is feedback-only.
                (props_are_affected, None, false)
            } else {
                // Numeric feedback implicitly always relates to the main target value, so we always
                // ask the target directly.
                let (main_target_value_is_affected, value) =
                    target.process_change_event(evt, context);
                (
                    main_target_value_is_affected || props_are_affected,
                    value,
                    true,
                )
            };
        if is_affected {
            let new_value = new_value.or_else(|| target.current_value(context));
            if handle_performance_mapping {
                self.update_last_non_performance_target_value_if_appropriate(new_value);
            }
            (true, new_value)
        } else {
            (false, None)
        }
    }

    pub fn update_last_non_performance_target_value_if_appropriate(
        &self,
        value: Option<AbsoluteValue>,
    ) {
        if let Some(v) = value {
            if self.control_is_enabled() && !self.is_echo() {
                self.last_non_performance_target_value.set(Some(v));
            }
        }
    }

    pub fn is_echo(&self) -> bool {
        self.core.is_echo()
    }

    pub fn update_last_non_performance_target_value(&self, value: AbsoluteValue) {
        self.last_non_performance_target_value.set(Some(value));
    }

    pub fn last_non_performance_target_value(&self) -> Option<AbsoluteValue> {
        self.last_non_performance_target_value.get()
    }

    pub fn take_mapping_info(&mut self) -> MappingInfo {
        MappingInfo {
            name: self.name.take().unwrap_or_default(),
        }
    }

    pub fn initial_target_value(&self) -> Option<AbsoluteValue> {
        self.initial_target_value
    }

    pub fn update_persistent_processing_state(&mut self, state: PersistentMappingProcessingState) {
        self.core.update_persistent_processing_state(state);
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn has_any_tag(&self, tags: &HashSet<Tag>) -> bool {
        self.tags.iter().any(|t| tags.contains(t))
    }

    pub fn qualified_source(&self) -> QualifiedSource {
        QualifiedSource {
            compartment: self.core.compartment,
            mapping_key: self.key.clone(),
            source: self.source().clone(),
        }
    }

    pub fn compartment(&self) -> Compartment {
        self.core.compartment
    }

    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn qualified_id(&self) -> QualifiedMappingId {
        QualifiedMappingId::new(self.core.compartment, self.core.id)
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn mode_control_options(&self) -> ModeControlOptions {
        ModeControlOptions {
            enforce_rotate: self.core.mode.settings().rotate,
        }
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
            is_active: self.is_active_in_terms_of_activation_state(),
            target_category: self.unresolved_target.as_ref().map(|t| match t {
                UnresolvedCompoundMappingTarget::Reaper(_) => UnresolvedTargetCategory::Reaper,
                UnresolvedCompoundMappingTarget::Virtual(_) => UnresolvedTargetCategory::Virtual,
            }),
            target_is_resolved: !self.targets.is_empty(),
            resolved_target: self.splinter_first_real_time_target(),
            lifecycle_midi_data: self
                .extension
                .lifecycle_midi_data
                .take()
                .unwrap_or_default(),
        }
    }

    pub fn splinter_first_real_time_target(&self) -> Option<RealTimeCompoundMappingTarget> {
        self.targets
            .first()
            .and_then(|t| t.splinter_real_time_target())
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
        matches!(
            self.unresolved_target,
            Some(UnresolvedCompoundMappingTarget::Reaper(_))
        )
    }

    pub fn has_resolved_successfully(&self) -> bool {
        !self.targets.is_empty()
    }

    pub fn check_activation_effect_of_target_value_update(
        &self,
        lead_mapping_id: MappingId,
        target_value: Option<AbsoluteValue>,
    ) -> Option<MappingActivationEffect> {
        let effect_1 = self
            .activation_condition_1
            .process_target_value_update(lead_mapping_id, target_value);
        let effect_2 = self
            .activation_condition_2
            .process_target_value_update(lead_mapping_id, target_value);
        MappingActivationEffect::new(self.id(), effect_1, effect_2)
    }

    /// Returns `Some` if this affects the mapping's activation state in any way.
    pub fn check_activation_effect_of_param_update(
        &self,
        params: &PluginParams,
        plugin_param_index: PluginParamIndex,
        previous_value: f32,
    ) -> Option<MappingActivationEffect> {
        let compartment_params = params.compartment_params(self.core.compartment);
        let compartment_param_index = self
            .core
            .compartment
            .to_compartment_param_index(plugin_param_index);
        let effect_1 = self.activation_condition_1.process_param_update(
            compartment_params,
            compartment_param_index,
            previous_value,
        );
        let effect_2 = self.activation_condition_2.process_param_update(
            compartment_params,
            compartment_param_index,
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

    /// Returns if the mapping's activation conditions can be affected by parameter changes in
    /// general.
    pub fn activation_can_be_affected_by_parameters(&self) -> bool {
        self.activation_condition_1.can_be_affected_by_parameters()
            || self.activation_condition_2.can_be_affected_by_parameters()
    }

    /// Returns if the mapping's activation conditions can be affected by target value changes
    /// of other mappings.
    ///
    /// In particular, it returns the IDs of the lead mappings (the ones which provide the
    /// target values that influence the activation state).
    pub fn activation_can_be_affected_by_target_values(&self) -> impl Iterator<Item = MappingId> {
        self.activation_condition_1
            .target_value_lead_mapping()
            .into_iter()
            .chain(
                self.activation_condition_2
                    .target_value_lead_mapping()
                    .into_iter(),
            )
    }

    pub fn update_activation_from_effect(
        &mut self,
        activation_effect: MappingActivationEffect,
    ) -> Option<RealTimeMappingUpdate> {
        let was_active_before = self.is_active_in_terms_of_activation_state();
        self.activation_state.is_active_1 = activation_effect
            .active_1_effect
            .unwrap_or(self.activation_state.is_active_1);
        self.activation_state.is_active_2 = activation_effect
            .active_2_effect
            .unwrap_or(self.activation_state.is_active_2);
        self.post_process_activation_update(was_active_before)
    }

    fn post_process_activation_update(
        &mut self,
        was_active_before: bool,
    ) -> Option<RealTimeMappingUpdate> {
        let now_is_active = self.is_active_in_terms_of_activation_state();
        if now_is_active == was_active_before {
            return None;
        }
        if !now_is_active {
            self.core.on_deactivate();
        }
        let update = RealTimeMappingUpdate {
            id: self.id(),
            activation_change: Some(ActivationChange {
                is_active: now_is_active,
            }),
        };
        Some(update)
    }

    pub fn init_target_and_activation(
        &mut self,
        context: ExtendedProcessorContext,
        control_context: ControlContext,
    ) {
        let (targets, is_active) = self.resolve_target(context, control_context);
        self.targets = targets;
        self.core.options.target_is_active = is_active;
        self.update_activation_from_params(context.params());
        let target_value = self.current_aggregated_target_value(control_context);
        self.initial_target_value = target_value;
        self.last_non_performance_target_value = Cell::new(target_value);
    }

    fn resolve_target(
        &mut self,
        context: ExtendedProcessorContext,
        control_context: ControlContext,
    ) -> (Vec<CompoundMappingTarget>, bool) {
        match self.unresolved_target.as_ref() {
            None => (vec![], false),
            Some(ut) => match ut.resolve(context, self.core.compartment).ok() {
                None => (vec![], false),
                Some(resolved_targets) => {
                    // Successfully resolved.
                    if let Some(t) = resolved_targets.first() {
                        // We have at least one target, great!
                        self.core.mode.update_from_target(t, control_context);
                        let met = ut.conditions_are_met(&resolved_targets);
                        (resolved_targets, met)
                    } else {
                        // Resolved to zero targets. Consider as inactive.
                        (vec![], false)
                    }
                }
            },
        }
    }

    pub fn needs_refresh_when_target_touched(&self) -> bool {
        matches!(
            self.unresolved_target,
            Some(UnresolvedCompoundMappingTarget::Reaper(
                UnresolvedReaperTarget::LastTouched(_)
            ))
        )
    }

    /// `None` means that no polling is necessary for feedback because we are notified via events.
    pub fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        let t = self.unresolved_target.as_ref()?;
        let max_resolution_required_by_props = self
            .core
            .mode
            .feedback_props_in_use()
            .iter()
            .filter_map(|p| prop_feedback_resolution(p, self, t))
            .max();
        if self.mode().wants_textual_feedback() {
            // For textual feedback, we just need to look at the props.
            max_resolution_required_by_props
        } else {
            // Numeric feedback always implicitly relates to the main target value, therefore
            // we also need to ask the target directly.
            t.feedback_resolution()
                .into_iter()
                .chain(max_resolution_required_by_props)
                .max()
        }
    }

    pub fn wants_to_be_polled_for_control(&self) -> bool {
        self.core.source.wants_to_be_polled() || self.core.mode.wants_to_be_polled()
    }

    /// The boolean return value tells if the resolved target changed in some way, the activation
    /// change says if activation changed from off to on or on to off.
    #[must_use]
    pub fn refresh_target(
        &mut self,
        context: ExtendedProcessorContext,
        control_context: ControlContext,
    ) -> Option<RealTimeTargetUpdate> {
        match self.unresolved_target.as_ref() {
            None => return None,
            Some(t) => {
                if !t.can_be_affected_by_change_events() {
                    return None;
                }
            }
        }
        let was_effectively_active_before = self.target_is_effectively_active();
        let (targets, is_active) = self.resolve_target(context, control_context);
        let target_changed = targets != self.targets;
        self.targets = targets;
        self.core.options.target_is_active = is_active;
        // Build real-time target update if necessary
        let activation_changed =
            self.target_is_effectively_active() != was_effectively_active_before;
        if !target_changed && !activation_changed {
            return None;
        }
        let update = RealTimeTargetUpdate {
            id: self.id(),
            activation_change: if activation_changed {
                Some(ActivationChange { is_active })
            } else {
                None
            },
            target_change: if target_changed {
                Some(self.splinter_first_real_time_target())
            } else {
                None
            },
        };
        Some(update)
    }

    pub fn update_activation_from_params(
        &mut self,
        params: &PluginParams,
    ) -> Option<RealTimeMappingUpdate> {
        let compartment_params = params.compartment_params(self.core.compartment);
        self.update_activation(
            self.activation_condition_1.is_fulfilled(compartment_params),
            self.activation_condition_2.is_fulfilled(compartment_params),
        )
    }

    fn update_activation(
        &mut self,
        is_active_1: Option<bool>,
        is_active_2: Option<bool>,
    ) -> Option<RealTimeMappingUpdate> {
        let was_active_before = self.is_active_in_terms_of_activation_state();
        if let Some(is_active) = is_active_1 {
            self.activation_state.is_active_1 = is_active;
        }
        if let Some(is_active) = is_active_2 {
            self.activation_state.is_active_2 = is_active;
        }
        self.post_process_activation_update(was_active_before)
    }

    /// Doesn't check if explicitly enabled or disabled.
    pub fn is_active_in_terms_of_activation_state(&self) -> bool {
        self.activation_state.is_active()
    }

    /// A target is considered as active if it resolves successfully to at least one real target
    /// and the target activation conditions (e.g. track must be selected) are met.
    pub fn target_is_active(&self) -> bool {
        self.core.options.target_is_active
    }

    /// Returns `true` if the mapping itself and the target is active.
    ///
    /// Doesn't check if explicitly enabled or disabled!
    pub fn is_effectively_active(&self) -> bool {
        is_effectively_active(
            &self.core.options,
            &self.activation_state,
            self.unresolved_target.as_ref(),
        )
    }

    fn target_is_effectively_active(&self) -> bool {
        target_is_effectively_active(&self.core.options, self.unresolved_target.as_ref())
    }

    /// Returns `true` if mapping & target is active and control or feedback is enabled.
    pub fn is_effectively_on(&self) -> bool {
        self.is_effectively_active() && (self.control_is_enabled() || self.feedback_is_enabled())
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.control_is_enabled()
    }

    pub fn control_is_enabled(&self) -> bool {
        self.core.options.control_is_effectively_enabled()
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.core.options.feedback_is_effectively_enabled()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        feedback_is_effectively_on(
            &self.core.options,
            &self.activation_state,
            self.unresolved_target.as_ref(),
        )
    }

    pub fn source(&self) -> &CompoundMappingSource {
        &self.core.source
    }

    pub fn targets(&self) -> &[CompoundMappingTarget] {
        &self.targets
    }

    /// This makes the button fire modes work (e.g. "Fire after delay").
    #[must_use]
    pub fn poll_mode(
        &mut self,
        context: ControlContext,
        logger: &slog::Logger,
        processor_context: ExtendedProcessorContext,
        timestamp: ControlEventTimestamp,
        log_mode_control_result: impl Fn(ControlLogEntry),
    ) -> MappingControlResult {
        self.control_internal(
            ControlOptions::default(),
            context,
            logger,
            processor_context,
            true,
            log_mode_control_result,
            |_, context, mode, target| mode.poll(target, context, timestamp),
        )
    }

    pub fn group_interaction(&self) -> GroupInteraction {
        self.core.group_interaction
    }

    /// Controls mode => target.
    ///
    /// Don't execute in real-time processor because this executes REAPER main-thread-only
    /// functions. If `send_feedback_after_control` is on, this might return feedback.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn control_from_mode(
        &mut self,
        source_control_event: ControlEvent<ControlValue>,
        options: ControlOptions,
        context: ControlContext,
        logger: &slog::Logger,
        processor_context: ExtendedProcessorContext,
        last_non_performance_target_value: Option<AbsoluteValue>,
        log_mode_control_result: impl Fn(ControlLogEntry),
    ) -> MappingControlResult {
        self.control_internal(
            options,
            context,
            logger,
            processor_context,
            false,
            log_mode_control_result,
            |options, context, mode, target| {
                mode.control_with_options(
                    source_control_event,
                    target,
                    context,
                    options.mode_control_options,
                    last_non_performance_target_value,
                )
            },
        )
    }

    /// Controls target directly without using mode.
    ///
    /// Don't execute in real-time processor because this executes REAPER main-thread-only
    /// functions. If `send_feedback_after_control` is on, this might return feedback.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn control_from_target_via_group_interaction(
        &mut self,
        value: AbsoluteValue,
        options: ControlOptions,
        context: ControlContext,
        logger: &slog::Logger,
        inverse: bool,
        processor_context: ExtendedProcessorContext,
        log_mode_control_result: impl Fn(ControlLogEntry),
    ) -> MappingControlResult {
        self.control_internal(
            options,
            context,
            logger,
            processor_context,
            false,
            log_mode_control_result,
            |_, _, mode, target| {
                let mut v = value;
                let control_type = target.control_type(context);
                // This is very similar to the mode logic, but just a small subset.
                if inverse {
                    let normalized_max = control_type.discrete_max().map(|m| {
                        mode.settings()
                            .discrete_target_value_interval
                            .normalize_to_min(m)
                    });
                    v = v.inverse(normalized_max);
                }
                v = v.denormalize(
                    &mode.settings().target_value_interval,
                    &mode.settings().discrete_target_value_interval,
                    mode.settings().use_discrete_processing,
                    control_type.discrete_max(),
                );
                Some(ModeControlResult::hit_target(ControlValue::from_absolute(
                    v,
                )))
            },
        )
    }

    fn data(&self) -> MappingData {
        MappingData {
            compartment: self.core.compartment,
            mapping_id: self.core.id,
            group_id: self.core.group_id,
            last_non_performance_target_value: self.last_non_performance_target_value(),
        }
    }

    #[must_use]
    pub fn control_from_target_directly(
        &mut self,
        context: ControlContext,
        logger: &slog::Logger,
        // TODO-low Strictly spoken, this is not necessary, because control_internal uses this only
        //  if target refresh is enforced, which is not the case here.
        processor_context: ExtendedProcessorContext,
        value: AbsoluteValue,
        log_mode_control_result: impl Fn(ControlLogEntry),
    ) -> MappingControlResult {
        self.control_internal(
            ControlOptions::default(),
            context,
            logger,
            processor_context,
            false,
            log_mode_control_result,
            |_, _, _, _| {
                Some(ModeControlResult::hit_target(ControlValue::from_absolute(
                    value,
                )))
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    fn control_internal(
        &mut self,
        options: ControlOptions,
        context: ControlContext,
        logger: &slog::Logger,
        processor_context: ExtendedProcessorContext,
        is_polling: bool,
        log_mode_control_result: impl Fn(ControlLogEntry),
        get_mode_control_result: impl Fn(
            ControlOptions,
            MappingControlContext,
            &mut Mode,
            &ReaperTarget,
        ) -> Option<ModeControlResult<ControlValue>>,
    ) -> MappingControlResult {
        let mut send_manual_feedback_because_of_target = false;
        let mut at_least_one_relevant_target_exists = false;
        let mut at_least_one_target_was_reached = false;
        let mut at_least_one_target_caused_effect = false;
        let mut first_hit_instruction = None;
        use ModeControlResult::*;
        let mut fresh_targets = if options.enforce_target_refresh {
            let (targets, conditions_are_met) = self.resolve_target(processor_context, context);
            if !conditions_are_met {
                return MappingControlResult::default();
            }
            targets
        } else {
            vec![]
        };
        let ctx = MappingControlContext {
            control_context: context,
            mapping_data: self.data(),
        };
        let actual_targets = if options.enforce_target_refresh {
            &mut fresh_targets
        } else {
            &mut self.targets
        };
        for target in actual_targets {
            let target = if let CompoundMappingTarget::Reaper(t) = target {
                t
            } else {
                continue;
            };
            at_least_one_relevant_target_exists = true;
            let (log_entry_kind, control_value, error) =
                match get_mode_control_result(options, ctx, &mut self.core.mode, target) {
                    None => {
                        // The incoming source value doesn't reach the target because the source value
                        // was filtered out. If `send_feedback_after_control` is enabled, we
                        // still send feedback - this can be useful with controllers which insist on
                        // controlling the LED on their own. The feedback sent by ReaLearn
                        // will fix this self-controlled LED state.
                        (ControlLogEntryKind::FilteredOutByGlue, None, "")
                    }
                    Some(HitTarget { value }) => {
                        at_least_one_target_was_reached = true;
                        if !is_polling {
                            self.core.time_of_last_control = Some(Instant::now());
                        }
                        // Be graceful here.
                        let (log_entry_kind, error) = match target.hit(value, ctx) {
                            Ok(response) => {
                                if response.caused_effect {
                                    at_least_one_target_caused_effect = true;
                                }
                                let log_entry_kind = if let Some(hi) = response.hit_instruction {
                                    // We have a hit instruction! Save it so it can be executed in
                                    // the next step.
                                    // TODO-low For now, the first hit instruction wins (at the moment we don't
                                    //  have multi-targets in which multiple targets send hit instructions
                                    //  anyway).
                                    if first_hit_instruction.is_none() {
                                        first_hit_instruction = Some(hi);
                                        ControlLogEntryKind::CreatedHitInstruction
                                    } else {
                                        ControlLogEntryKind::DiscardedHitInstruction
                                    }
                                } else if response.caused_effect {
                                    ControlLogEntryKind::HitSuccessfully
                                } else {
                                    ControlLogEntryKind::Ignored
                                };
                                (log_entry_kind, "")
                            }
                            Err(msg) => {
                                slog::debug!(logger, "Control failed: {}", msg);
                                (ControlLogEntryKind::HitFailed, msg)
                            }
                        };
                        if should_send_manual_feedback_due_to_target(
                            target,
                            &self.core.options,
                            &self.activation_state,
                            self.unresolved_target.as_ref(),
                        ) {
                            send_manual_feedback_because_of_target = true;
                        }
                        (log_entry_kind, Some(value), error)
                    }
                    Some(LeaveTargetUntouched(v)) => {
                        // The target already has the desired value.
                        // If `send_feedback_after_control` is enabled, we still send feedback - this
                        // can be useful with controllers which insist on controlling the LED on their
                        // own. The feedback sent by ReaLearn will fix this self-controlled LED state.
                        at_least_one_target_was_reached = true;
                        (ControlLogEntryKind::LeftTargetUntouched, Some(v), "")
                    }
                };
            // Log
            let log_entry = ControlLogEntry {
                kind: log_entry_kind,
                control_value,
                error,
            };
            log_mode_control_result(log_entry);
        }
        if send_manual_feedback_because_of_target {
            let new_target_value = self.current_aggregated_target_value(context);
            MappingControlResult {
                at_least_one_target_was_reached,
                at_least_one_target_caused_effect,
                new_target_value,
                feedback_value: self.manual_feedback_because_of_target(new_target_value, context),
                hit_instruction: first_hit_instruction,
                celebrate_success: self.beep_on_success(),
            }
        } else {
            MappingControlResult {
                at_least_one_target_was_reached,
                at_least_one_target_caused_effect,
                new_target_value: None,
                feedback_value: if !is_polling && at_least_one_relevant_target_exists {
                    // Before #396, we only sent "feedback after control" if the target was not hit at all.
                    // Reasoning was that if the target was hit, there must have been a value change
                    // (because we usually don't hit a target if it already has the desired value)
                    // and this value change would cause automatic feedback anyway. Then it wouldn't
                    // be necessary to send additional manual feedback.
                    //
                    // But this conclusion is wrong in some cases:
                    // 1. The target value might be very, very close to the desired value but not
                    //    the same. The target would be hit then (for being safe) but no feedback
                    //    might be generated because the difference might be insignificant regarding
                    //    our FEEDBACK_EPSILON (checked when polling feedback). This also depends a
                    //    bit on how the target interprets super-tiny value changes.
                    // 2. If we have a retriggerable target, we would always hit it, even if its
                    //    value wouldn't change.
                    //
                    // The new strategy is: Better redundant feedback messages than omitting
                    // important ones. This is just a workaround for weird controllers anyway!
                    // At the very least they should be able to cope with a few more feedback
                    // messages.
                    // TODO-bkl-medium we could optimize this in future by checking
                    //  significance of the difference within the mapping (should be easy now that
                    //  we have mutable access to self here).
                    self.manual_feedback_after_control_if_enabled(options, context)
                } else {
                    None
                },
                hit_instruction: first_hit_instruction,
                celebrate_success: self.beep_on_success(),
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

    fn manual_feedback_because_of_target(
        &self,
        new_target_value: Option<AbsoluteValue>,
        control_context: ControlContext,
    ) -> Option<CompoundFeedbackValue> {
        self.feedback_entry_point(true, true, new_target_value?, control_context)
            .map(CompoundFeedbackValue::normal)
    }

    /// Returns `None` when used on mappings with virtual targets.
    pub fn feedback(
        &self,
        with_projection_feedback: bool,
        context: ControlContext,
    ) -> Option<CompoundFeedbackValue> {
        self.feedback_entry_point(
            with_projection_feedback,
            true,
            self.current_aggregated_target_value(context)?,
            context,
        )
        .map(CompoundFeedbackValue::normal)
    }

    /// This is the primary entry point to feedback!
    ///
    /// Returns `None` when used on mappings with virtual targets.
    pub fn feedback_entry_point(
        &self,
        with_projection_feedback: bool,
        with_source_feedback: bool,
        combined_target_value: AbsoluteValue,
        control_context: ControlContext,
    ) -> Option<SpecificCompoundFeedbackValue> {
        // - We shouldn't ask the source if it wants the given numerical feedback value or a textual
        //   value because a virtual source wouldn't know! Even asking a real source wouldn't make
        //   much sense because real sources could be capable of processing both numerical and
        //   textual feedback (and indeed that makes sense for an LCD source!).
        // - Neither should we ask the target because the target is not supposed to dictate which
        //   form of feedback it sends, it just provides us with options and we can choose.
        // - This leaves us with asking the mode. That means the user needs to explicitly choose
        //   whether it wants numerical or textual feedback.
        let feedback_value = if self.core.mode.wants_textual_feedback() {
            let v = self
                .core
                .mode
                .query_textual_feedback(&|key| get_prop_value(key, self, control_context));
            FeedbackValue::Textual(v)
        } else {
            let style = self
                .core
                .mode
                .feedback_style(&|key| get_prop_value(key, self, control_context));
            FeedbackValue::Numeric(NumericFeedbackValue::new(style, combined_target_value))
        };
        let source_feedback_is_okay = if self.core.options.feedback_send_behavior
            == FeedbackSendBehavior::PreventEchoFeedback
        {
            !self.core.is_echo()
        } else {
            true
        };
        self.feedback_given_target_value(
            Cow::Owned(feedback_value),
            FeedbackDestinations {
                with_projection_feedback,
                with_source_feedback: with_source_feedback && source_feedback_is_okay,
            },
            control_context.source_context,
        )
    }

    pub fn current_aggregated_target_value(
        &self,
        context: ControlContext,
    ) -> Option<AbsoluteValue> {
        let values = self.targets.iter().map(|t| t.current_value(context));
        aggregate_target_values(values)
    }

    pub fn mode(&self) -> &Mode {
        &self.core.mode
    }

    pub fn group_id(&self) -> GroupId {
        self.core.group_id
    }

    /// Taking the feedback value as a Cow is better than taking a reference because with a
    /// reference we would for sure have to clone a textual feedback value, even if the consumer
    /// can give us ownership of the feedback value. It's also better than taking an owned value
    /// because it's possible that we don't produce a feedback value at all! In which a consumer
    /// that can't give up ownership would need to make a clone in advance - for nothing!
    pub fn feedback_given_target_value(
        &self,
        feedback_value: Cow<FeedbackValue>,
        destinations: FeedbackDestinations,
        source_context: &SourceContext,
    ) -> Option<SpecificCompoundFeedbackValue> {
        let options = ModeFeedbackOptions {
            source_is_virtual: self.core.source.is_virtual(),
            max_discrete_source_value: self.core.source.max_discrete_value(),
        };
        let mode_value = self.core.mode.feedback_with_options_detail(
            feedback_value,
            options,
            Default::default(),
        )?;
        self.feedback_given_mode_value(mode_value, destinations, source_context)
    }

    fn feedback_given_mode_value(
        &self,
        mode_value: Cow<FeedbackValue>,
        destinations: FeedbackDestinations,
        source_context: &SourceContext,
    ) -> Option<SpecificCompoundFeedbackValue> {
        SpecificCompoundFeedbackValue::from_mode_value(
            self.core.compartment,
            self.key.clone(),
            &self.core.source,
            mode_value,
            destinations,
            source_context,
        )
    }

    /// This returns a "lights off" feedback.
    ///
    /// Used when mappings get inactive.
    pub fn off_feedback(&self, source_context: &SourceContext) -> Option<CompoundFeedbackValue> {
        // TODO-medium  "Unused" and "zero" could be a difference for projection so we should
        //  have different values for that (at the moment it's not though).
        self.feedback_given_mode_value(
            Cow::Owned(FeedbackValue::Off),
            FeedbackDestinations {
                with_projection_feedback: true,
                with_source_feedback: true,
            },
            source_context,
        )
        .map(CompoundFeedbackValue::normal)
    }

    fn manual_feedback_after_control_if_enabled(
        &self,
        options: ControlOptions,
        context: ControlContext,
    ) -> Option<CompoundFeedbackValue> {
        if self.core.options.feedback_send_behavior
            == FeedbackSendBehavior::SendFeedbackAfterControl
            || options.enforce_send_feedback_after_control
        {
            if self.feedback_is_effectively_on() {
                // No projection feedback in this case! Just the source controller needs this hack.
                self.feedback_entry_point(
                    false,
                    true,
                    self.current_aggregated_target_value(context)?,
                    context,
                )
                .map(CompoundFeedbackValue::feedback_after_control)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Controls the source only.
    ///
    /// Doesn't consider MIDI sources because they are handled completely in the real-time mapping.
    pub fn control_source(
        &mut self,
        msg: MainSourceMessage,
    ) -> Option<ControlOutcome<ControlValue>> {
        let compartment = self.compartment();
        match (msg, &mut self.core.source) {
            (MainSourceMessage::Osc(m), CompoundMappingSource::Osc(s)) => {
                // With OSC sources, we don't distinguish between matched or consumed because
                // there's no such thing such as "letting messages through".
                s.control(m).map(ControlOutcome::Matched)
            }
            (MainSourceMessage::Reaper(m), CompoundMappingSource::Reaper(s)) => {
                // With REAPER sources, we don't distinguish between matched or consumed because
                // there's no such thing such as "letting messages through".
                s.control(m, compartment).map(ControlOutcome::Matched)
            }
            (MainSourceMessage::Key(m), CompoundMappingSource::Key(s)) => s.control(m),
            _ => None,
        }
    }

    /// Polls the source.
    pub fn poll_source(&mut self) -> Option<ControlValue> {
        match &mut self.core.source {
            CompoundMappingSource::Reaper(s) => s.poll(),
            _ => None,
        }
    }

    pub fn control_virtualizing(
        &mut self,
        evt: ControlEvent<MainSourceMessage>,
    ) -> Option<ControlOutcome<VirtualSourceValue>> {
        if self.targets.is_empty() {
            return None;
        }
        let control_value = match self.control_source(evt.payload())? {
            ControlOutcome::Consumed => {
                return Some(ControlOutcome::Consumed);
            }
            ControlOutcome::Matched(v) => v,
        };
        // First target is enough because this does nothing yet.
        let virtual_source_value = match self.targets.first()? {
            CompoundMappingTarget::Virtual(t) => {
                match_partially(&mut self.core, t, evt.with_payload(control_value))
            }
            CompoundMappingTarget::Reaper(_) => None,
        };
        Some(ControlOutcome::Matched(virtual_source_value?))
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MainSourceMessage<'a> {
    Osc(&'a OscMessage),
    Reaper(&'a ReaperMessage),
    Key(KeyMessage),
}

impl<'a> MainSourceMessage<'a> {
    /// Extracts data if this kind of message supports source learning, filtering etc., otherwise
    /// returns `None`.
    pub fn create_capture_result(&self) -> Option<MessageCaptureResult> {
        use MainSourceMessage::*;
        let res = match *self {
            Osc(msg) => MessageCaptureResult::Osc(OscScanResult {
                message: msg.clone(),
                dev_id: None,
            }),
            Key(msg) => MessageCaptureResult::Keyboard(msg),
            Reaper(msg) => {
                use ReaperMessage::*;
                match msg {
                    MidiDevicesConnected(_)
                    | MidiDevicesDisconnected(_)
                    | RealearnInstanceStarted => return None,
                    RealearnParameterChange(payload) => {
                        MessageCaptureResult::RealearnParameter(*payload)
                    }
                }
            }
        };
        Some(res)
    }
}

#[derive(Debug)]
pub struct RealTimeMapping {
    pub core: MappingCore,
    is_active: bool,
    /// Is `Some` if user-provided target data is complete.
    target_category: Option<UnresolvedTargetCategory>,
    target_is_resolved: bool,
    /// Is `Some` if virtual or this target needs to be processed in real-time.
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
        if v {
            Activation
        } else {
            Deactivation
        }
    }
}

impl RealTimeMapping {
    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn compartment(&self) -> Compartment {
        self.core.compartment
    }

    pub fn lifecycle_midi_messages(&self, phase: LifecyclePhase) -> &[LifecycleMidiMessage] {
        use LifecyclePhase::*;
        match phase {
            Activation => &self.lifecycle_midi_data.activation_midi_messages,
            Deactivation => &self.lifecycle_midi_data.deactivation_midi_messages,
        }
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.control_is_enabled()
    }

    pub fn control_is_enabled(&self) -> bool {
        self.core.options.control_is_effectively_enabled()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.is_effectively_active() && self.core.options.feedback_is_effectively_enabled()
    }

    pub fn feedback_is_effectively_on_ignoring_mapping_activation(&self) -> bool {
        self.is_effectively_active_ignoring_mapping_activation()
            && self.core.options.feedback_is_effectively_enabled()
    }

    pub fn feedback_is_effectively_on_ignoring_target_activation(&self) -> bool {
        self.is_effectively_active_ignoring_target_activation()
            && self.core.options.feedback_is_effectively_enabled()
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

    pub fn update_persistent_processing_state(&mut self, state: PersistentMappingProcessingState) {
        self.core.update_persistent_processing_state(state);
    }

    pub fn update_target(&mut self, update: &mut RealTimeTargetUpdate) {
        if let Some(c) = update.activation_change {
            self.core.options.target_is_active = c.is_active;
        }
        if let Some(rt_target) = update.target_change.take() {
            self.resolved_target = rt_target;
        }
    }

    pub fn update(&mut self, update: &RealTimeMappingUpdate) {
        if let Some(c) = update.activation_change {
            let was_active_before = self.is_active;
            self.is_active = c.is_active;
            if was_active_before && !c.is_active {
                self.core.on_deactivate();
            }
        }
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
        ModeControlOptions {
            enforce_rotate: self.core.mode.settings().rotate,
        }
    }

    pub fn control_midi_virtualizing(
        &mut self,
        evt: ControlEvent<&MidiSourceValue<RawShortMessage>>,
    ) -> Option<PartialControlMatch> {
        if !self.target_is_resolved {
            return None;
        }
        let control_value = if let CompoundMappingSource::Midi(s) = &self.core.source {
            s.control(evt.payload())?
        } else {
            return None;
        };
        if let Some(RealTimeCompoundMappingTarget::Virtual(t)) = self.resolved_target.as_ref() {
            match_partially(&mut self.core, t, evt.with_payload(control_value))
                .map(PartialControlMatch::ProcessVirtual)
        } else {
            Some(PartialControlMatch::ProcessDirect(control_value))
        }
    }
}

pub enum PartialControlMatch {
    ProcessVirtual(VirtualSourceValue),
    ProcessDirect(ControlValue),
}

#[derive(Clone, Debug)]
pub struct MappingCore {
    compartment: Compartment,
    id: MappingId,
    group_id: GroupId,
    pub source: CompoundMappingSource,
    pub mode: Mode,
    group_interaction: GroupInteraction,
    options: ProcessorMappingOptions,
    /// Used for preventing echo feedback.
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

    fn update_persistent_processing_state(&mut self, state: PersistentMappingProcessingState) {
        let was_enabled_before = self.options.persistent_processing_state.is_enabled;
        self.options.persistent_processing_state = state;
        if was_enabled_before && !state.is_enabled {
            self.on_deactivate();
        }
    }

    fn on_deactivate(&mut self) {
        self.source.on_deactivate();
        self.mode.on_deactivate();
    }
}

// PartialEq because we want to put it into a Prop.
#[derive(Clone, PartialEq, Debug)]
pub enum CompoundMappingSource {
    Never,
    Midi(MidiSource),
    Osc(OscSource),
    Virtual(VirtualSource),
    Reaper(ReaperSource),
    Key(KeySource),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum CompoundMappingSourceAddress {
    Midi(MidiSourceAddress),
    Osc(OscSourceAddress),
    Virtual(VirtualSourceAddress),
}

#[derive(Clone, Debug)]
pub struct QualifiedSource {
    pub compartment: Compartment,
    pub mapping_key: Rc<str>,
    pub source: CompoundMappingSource,
}

impl QualifiedSource {
    pub fn off_feedback(self, source_context: &SourceContext) -> Option<CompoundFeedbackValue> {
        SpecificCompoundFeedbackValue::from_mode_value(
            self.compartment,
            self.mapping_key,
            &self.source,
            Cow::Owned(FeedbackValue::Off),
            FeedbackDestinations {
                with_projection_feedback: true,
                with_source_feedback: true,
            },
            source_context,
        )
        .map(CompoundFeedbackValue::normal)
    }
}

impl CompoundMappingSource {
    /// This should be called when the containing mapping gets deactivated.
    ///
    /// Attention: At the moment it can be called even if the mapping was already inactive.
    /// So it should be idempotent!
    #[allow(clippy::single_match)]
    pub fn on_deactivate(&mut self) {
        use CompoundMappingSource::*;
        match self {
            Reaper(s) => s.on_deactivate(),
            _ => {}
        }
    }

    /// If this returns `true`, the `poll` method should be called, on a regular basis.
    pub fn wants_to_be_polled(&self) -> bool {
        use CompoundMappingSource::*;
        match self {
            Reaper(s) => s.wants_to_be_polled(),
            _ => false,
        }
    }

    /// Extracts the address of the source control element for feedback purposes.
    ///
    /// Use this if you really need an owned representation of the source address. If you just want
    /// to compare addresses, use [`Self::has_same_feedback_address_as_value`]
    /// or [`Self::has_same_feedback_address_as_source`] instead. It can avoid the cloning.
    // TODO-medium There are quite some places in which we are fine with a borrowed version but
    //  the problem is the MIDI source can't simply give us a borrowed one. Maybe we should
    //  create one at MIDI source creation time! But for this we need to make MidiSource a struct.
    pub fn extract_feedback_address(&self) -> Option<CompoundMappingSourceAddress> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => Some(CompoundMappingSourceAddress::Midi(
                s.extract_feedback_address()?,
            )),
            Osc(s) => Some(CompoundMappingSourceAddress::Osc(
                s.feedback_address().clone(),
            )),
            Virtual(s) => Some(CompoundMappingSourceAddress::Virtual(*s.feedback_address())),
            _ => None,
        }
    }

    /// Checks if the given message is directed to the same address as the one of this source.
    ///
    /// Used for:
    ///
    /// -  Source takeover (feedback)
    pub fn has_same_feedback_address_as_value(&self, value: &FinalSourceFeedbackValue) -> bool {
        use CompoundMappingSource::*;
        match (self, value) {
            (Osc(s), FinalSourceFeedbackValue::Osc(v)) => s.has_same_feedback_address_as_value(v),
            (Midi(s), FinalSourceFeedbackValue::Midi(v)) => s.has_same_feedback_address_as_value(v),
            _ => false,
        }
    }

    /// Checks if this and the given source share the same address.
    ///
    /// Used for:
    ///
    /// - Feedback diffing
    pub fn has_same_feedback_address_as_source(&self, other: &Self) -> bool {
        use CompoundMappingSource::*;
        match (self, other) {
            (Osc(s1), Osc(s2)) => s1.has_same_feedback_address_as_source(s2),
            (Midi(s1), Midi(s2)) => s1.has_same_feedback_address_as_source(s2),
            (Virtual(s1), Virtual(s2)) => s1.has_same_feedback_address_as_source(s2),
            _ => false,
        }
    }

    /// Can be used to check if this mapping would react to the given message.
    ///
    /// The important difference to controlling is that it doesn't mutate the source.
    ///
    /// Used for:
    ///
    /// - Source learning (including source virtualization)
    /// - Source filtering/finding (including source virtualization)
    pub fn reacts_to_source_value_with(
        &self,
        value: IncomingCompoundSourceValue,
    ) -> Option<ControlResult> {
        use CompoundMappingSource::*;
        match (self, value) {
            (Midi(s), IncomingCompoundSourceValue::Midi(v)) => s.control_flexible(v),
            (Osc(s), IncomingCompoundSourceValue::Osc(m)) => {
                s.control(m).map(ControlResult::Processed)
            }
            (Virtual(s), IncomingCompoundSourceValue::Virtual(m)) => {
                s.control(m).map(ControlResult::Processed)
            }
            (Key(s), IncomingCompoundSourceValue::Key(m)) => {
                s.reacts_to_message_with(m).map(ControlResult::Processed)
            }
            _ => None,
        }
    }

    pub fn from_message_capture_event(event: MessageCaptureEvent) -> Option<Self> {
        use MessageCaptureResult::*;
        let res = match event.result {
            Midi(scan_result) => {
                let midi_source =
                    MidiSource::from_source_value(scan_result.value, scan_result.character)?;
                Self::Midi(midi_source)
            }
            Osc(msg) => {
                let osc_source =
                    OscSource::from_source_value(msg.message, event.osc_arg_index_hint);
                Self::Osc(osc_source)
            }
            Keyboard(msg) => {
                let key_source = KeySource::new(msg.stroke());
                Self::Key(key_source)
            }
            RealearnParameter(payload) => {
                let reaper_source = ReaperSource::RealearnParameter(RealearnParameterSource {
                    parameter_index: payload.parameter_index,
                });
                Self::Reaper(reaper_source)
            }
        };
        Some(res)
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.format_control_value(value),
            Virtual(s) => s.format_control_value(value),
            Osc(s) => s.format_control_value(value),
            Reaper(s) => s.format_control_value(value),
            Never | Key(_) => Ok(format_percentage_without_unit(value.to_unit_value()?.get())),
        }
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.parse_control_value(text),
            Virtual(s) => s.parse_control_value(text),
            Osc(s) => s.parse_control_value(text),
            Reaper(s) => s.parse_control_value(text),
            Never | Key(_) => parse_percentage_without_unit(text)?.try_into(),
        }
    }

    pub fn character(&self) -> ExtendedSourceCharacter {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => ExtendedSourceCharacter::Normal(s.character()),
            Virtual(s) => s.character(),
            Osc(s) => ExtendedSourceCharacter::Normal(s.character()),
            Reaper(s) => ExtendedSourceCharacter::Normal(s.character()),
            Never => ExtendedSourceCharacter::VirtualContinuous,
            Key(_) => ExtendedSourceCharacter::Normal(SourceCharacter::MomentaryButton),
        }
    }

    pub fn feedback(
        &self,
        feedback_value: Cow<FeedbackValue>,
        source_context: &SourceContext,
    ) -> Option<PreliminarySourceFeedbackValue> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s
                .feedback_flexible(feedback_value.into_owned(), source_context)
                .map(PreliminarySourceFeedbackValue::Midi),
            Osc(s) => s
                .feedback(feedback_value.into_owned())
                .map(PreliminarySourceFeedbackValue::Osc),
            // This is handled in a special way by consumers.
            Virtual(_) => None,
            // No feedback for never source.
            Reaper(_) | Key(_) | Never => None,
        }
    }

    pub fn consumes(&self, msg: &impl ShortMessage) -> bool {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.consumes(msg),
            Reaper(_) | Virtual(_) | Osc(_) | Never | Key(_) => false,
        }
    }

    pub fn is_virtual(&self) -> bool {
        matches!(self, CompoundMappingSource::Virtual(_))
    }

    pub fn max_discrete_value(&self) -> Option<u32> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.max_discrete_value(),
            // TODO-medium OSC will also support discrete values as soon as we allow integers and
            //  configuring max values
            Reaper(_) | Virtual(_) | Osc(_) | Never | Key(_) => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CompoundFeedbackValue {
    pub value: SpecificCompoundFeedbackValue,
    pub is_feedback_after_control: bool,
}

impl CompoundFeedbackValue {
    pub fn normal(value: SpecificCompoundFeedbackValue) -> Self {
        Self {
            value,
            is_feedback_after_control: false,
        }
    }

    pub fn feedback_after_control(value: SpecificCompoundFeedbackValue) -> Self {
        Self {
            value,
            is_feedback_after_control: true,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum SpecificCompoundFeedbackValue {
    Virtual {
        value: VirtualFeedbackValue,
        destinations: FeedbackDestinations,
    },
    Real(PreliminaryRealFeedbackValue),
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct FeedbackDestinations {
    /// Feedback to projection clients.
    pub with_projection_feedback: bool,
    /// Feedback to controller itself.
    pub with_source_feedback: bool,
}

impl FeedbackDestinations {
    pub fn is_all_off(&self) -> bool {
        !self.with_source_feedback && !self.with_projection_feedback
    }
}

impl SpecificCompoundFeedbackValue {
    pub fn from_mode_value(
        compartment: Compartment,
        mapping_key: Rc<str>,
        source: &CompoundMappingSource,
        mode_value: Cow<FeedbackValue>,
        destinations: FeedbackDestinations,
        source_context: &SourceContext,
    ) -> Option<SpecificCompoundFeedbackValue> {
        if destinations.is_all_off() {
            return None;
        }
        let val = if let CompoundMappingSource::Virtual(vs) = &source {
            // Virtual source
            SpecificCompoundFeedbackValue::Virtual {
                destinations,
                value: vs.feedback(mode_value.into_owned()),
            }
        } else {
            // Real source
            let projection = if destinations.with_projection_feedback
                && compartment == Compartment::Controller
            {
                // TODO-medium Support textual projection feedback
                mode_value.to_numeric().map(|v| {
                    ProjectionFeedbackValue::new(compartment, mapping_key, v.value.to_unit_value())
                })
            } else {
                None
            };
            let source = if destinations.with_source_feedback {
                source.feedback(mode_value, source_context)
            } else {
                None
            };
            SpecificCompoundFeedbackValue::Real(PreliminaryRealFeedbackValue::new(
                projection, source,
            )?)
        };
        Some(val)
    }
}

pub type PreliminaryRealFeedbackValue = AbstractRealFeedbackValue<PreliminarySourceFeedbackValue>;
pub type FinalRealFeedbackValue = AbstractRealFeedbackValue<FinalSourceFeedbackValue>;

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AbstractRealFeedbackValue<T> {
    /// Feedback to be sent to projection.
    ///
    /// This is an option because there are situations when we don't want projection feedback but
    /// source feedback (e.g. for "Feedback after control" because of too clever controllers).
    pub projection: Option<ProjectionFeedbackValue>,
    /// Feedback to be sent to the source.
    ///
    /// This is an option because there are situations when we don't want source feedback but
    /// projection feedback (e.g. if "MIDI feedback output" is set to None).
    pub source: Option<T>,
}

impl<T> AbstractRealFeedbackValue<T> {
    pub fn new(projection: Option<ProjectionFeedbackValue>, source: Option<T>) -> Option<Self> {
        if projection.is_none() && source.is_none() {
            return None;
        }
        let val = Self { projection, source };
        Some(val)
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ProjectionFeedbackValue {
    pub compartment: Compartment,
    pub mapping_key: Rc<str>,
    pub value: UnitValue,
}

impl ProjectionFeedbackValue {
    pub fn new(compartment: Compartment, mapping_key: Rc<str>, value: UnitValue) -> Self {
        Self {
            compartment,
            mapping_key,
            value,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum PreliminarySourceFeedbackValue {
    Midi(PreliminaryMidiSourceFeedbackValue<'static, RawShortMessage>),
    Osc(OscMessage),
}

#[derive(Clone, PartialEq, Debug)]
pub enum FinalSourceFeedbackValue {
    Midi(MidiSourceValue<'static, RawShortMessage>),
    Osc(OscMessage),
}

impl FinalSourceFeedbackValue {
    pub fn extract_address(&self) -> Option<CompoundMappingSourceAddress> {
        match self {
            FinalSourceFeedbackValue::Midi(v) => v
                .extract_feedback_address()
                .map(CompoundMappingSourceAddress::Midi),
            FinalSourceFeedbackValue::Osc(v) => {
                Some(CompoundMappingSourceAddress::Osc(v.addr.clone()))
            }
        }
    }
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
        compartment: Compartment,
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

    pub fn can_be_affected_by_change_events(&self) -> bool {
        use UnresolvedCompoundMappingTarget::*;
        match self {
            Reaper(t) => t.can_be_affected_by_change_events(),
            Virtual(_) => false,
        }
    }

    /// `None` means that no polling is necessary for feedback because we are notified via events.
    pub fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        use UnresolvedCompoundMappingTarget::*;
        match self {
            Reaper(t) => t.feedback_resolution(),
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

    pub fn is_virtual(&self) -> bool {
        matches!(self, CompoundMappingTarget::Virtual(_))
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum RealTimeCompoundMappingTarget {
    Reaper(RealTimeReaperTarget),
    Virtual(VirtualTarget),
}

pub struct WithControlContext<'a, T> {
    control_context: ControlContext<'a>,
    value: &'a T,
}

impl<'a, T> WithControlContext<'a, T> {
    pub fn new(control_context: ControlContext<'a>, value: &'a T) -> Self {
        Self {
            control_context,
            value,
        }
    }
}

impl<'a> ValueFormatter for WithControlContext<'a, CompoundMappingTarget> {
    fn format_value(&self, value: UnitValue, f: &mut Formatter) -> fmt::Result {
        f.write_str(
            &self
                .value
                .format_value_without_unit(value, self.control_context),
        )
    }

    fn format_step(&self, value: UnitValue, f: &mut Formatter) -> fmt::Result {
        f.write_str(
            &self
                .value
                .format_step_size_without_unit(value, self.control_context),
        )
    }
}

impl<'a> ValueParser for WithControlContext<'a, CompoundMappingTarget> {
    fn parse_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.value.parse_as_value(text, self.control_context)
    }

    fn parse_step(&self, text: &str) -> Result<UnitValue, &'static str> {
        self.value.parse_as_step_size(text, self.control_context)
    }
}

impl RealearnTarget for CompoundMappingTarget {
    fn character(&self, context: ControlContext) -> TargetCharacter {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.character(context),
            Virtual(t) => t.character(),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.text_value(context),
            Virtual(_) => None,
        }
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.numeric_value(context),
            Virtual(_) => None,
        }
    }

    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control_type_and_character(context),
            Virtual(t) => (t.control_type(()), t.character()),
        }
    }

    fn open(&self, context: ControlContext) {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.open(context),
            Virtual(_) => {}
        };
    }
    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.parse_as_value(text, context),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.parse_as_step_size(text, context),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.convert_unit_value_to_discrete_value(input, context),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_value_without_unit(value, context),
            Virtual(_) => String::new(),
        }
    }

    fn format_step_size_without_unit(
        &self,
        step_size: UnitValue,
        context: ControlContext,
    ) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_step_size_without_unit(step_size, context),
            Virtual(_) => String::new(),
        }
    }

    fn hide_formatted_value(&self, context: ControlContext) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.hide_formatted_value(context),
            Virtual(_) => false,
        }
    }

    fn hide_formatted_step_size(&self, context: ControlContext) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.hide_formatted_step_size(context),
            Virtual(_) => false,
        }
    }

    fn value_unit(&self, context: ControlContext) -> &'static str {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.value_unit(context),
            Virtual(_) => "",
        }
    }

    fn step_size_unit(&self, context: ControlContext) -> &'static str {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.step_size_unit(context),
            Virtual(_) => "",
        }
    }

    fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.format_value(value, context),
            Virtual(_) => String::new(),
        }
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.hit(value, context),
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

    fn is_available(&self, context: ControlContext) -> bool {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.is_available(context),
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
        evt: CompoundChangeEvent,
        control_context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        // TODO-medium I think this abstraction is not in use
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.process_change_event(evt, control_context),
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

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.convert_discrete_value_to_unit_value(value, context),
            Virtual(_) => Err("not supported for virtual targets"),
        }
    }

    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.prop_value(key, context),
            Virtual(_) => None,
        }
    }

    fn numeric_value_unit(&self, context: ControlContext) -> &'static str {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.numeric_value_unit(context),
            Virtual(_) => "",
        }
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.reaper_target_type(),
            Virtual(_) => None,
        }
    }
}

impl<'a> Target<'a> for CompoundMappingTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext) -> Option<AbsoluteValue> {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.current_value(context),
            Virtual(t) => t.current_value(()),
        }
    }

    fn control_type(&self, context: ControlContext) -> ControlType {
        use CompoundMappingTarget::*;
        match self {
            Reaper(t) => t.control_type(context),
            Virtual(t) => t.control_type(()),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct QualifiedMappingId {
    pub compartment: Compartment,
    pub id: MappingId,
}

impl QualifiedMappingId {
    pub fn new(compartment: Compartment, id: MappingId) -> Self {
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
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum Compartment {
    // It's important for `RealTimeProcessor` logic that this is the first element! We use array
    // destructuring.
    #[display(fmt = "controller compartment")]
    Controller,
    #[display(fmt = "main compartment")]
    Main,
}

impl Compartment {
    /// We could also use the generated `into_enum_iter()` everywhere but IDE completion
    /// in IntelliJ Rust doesn't work for that at the time of this writing.
    pub fn enum_iter() -> impl Iterator<Item = Compartment> + ExactSizeIterator {
        Compartment::into_enum_iter()
    }

    /// Returns the compartment to which the given plug-in parameter index belongs.
    pub fn by_plugin_param_index(plugin_param_index: PluginParamIndex) -> Compartment {
        Self::enum_iter()
            .find(|c| c.plugin_param_range().contains(&plugin_param_index))
            .unwrap()
    }

    /// Translates the given plug-in parameter index to a compartment-local index.
    pub fn translate_plugin_param_index(
        index: PluginParamIndex,
    ) -> (Compartment, CompartmentParamIndex) {
        let compartment = Self::by_plugin_param_index(index);
        (compartment, compartment.to_compartment_param_index(index))
    }

    /// Returns the compartment-local parameter index corresponding to the given plug-in parameter
    /// index.
    pub fn to_compartment_param_index(
        self,
        plugin_param_index: PluginParamIndex,
    ) -> CompartmentParamIndex {
        CompartmentParamIndex::try_from(plugin_param_index.get() - self.plugin_param_offset().get())
            .unwrap()
    }

    /// Returns the plug-in parameter range corresponding to this compartment.
    pub fn plugin_param_range(self) -> RangeInclusive<PluginParamIndex> {
        let offset = self.plugin_param_offset();
        offset..=(offset + (COMPARTMENT_PARAMETER_COUNT - 1)).unwrap()
    }

    fn plugin_param_offset(self) -> PluginParamIndex {
        let raw_offset = match self {
            Compartment::Controller => 100u32,
            Compartment::Main => 0u32,
        };
        PluginParamIndex::try_from(raw_offset).unwrap()
    }
}

pub enum ExtendedSourceCharacter {
    Normal(SourceCharacter),
    VirtualContinuous,
}

fn match_partially(
    core: &mut MappingCore,
    target: &VirtualTarget,
    control_event: ControlEvent<ControlValue>,
) -> Option<VirtualSourceValue> {
    // Determine resulting virtual control value in real-time processor.
    // It's important to do that here. We need to know the result in order to
    // return if there was actually a match of *real* non-virtual mappings.
    // Unlike with REAPER targets, we also don't have threading issues here :)
    // TODO-medium If we want to support fire after timeout and turbo for mappings with
    //  virtual targets one day, we need to poll this in real-time processor and OSC
    //  processing, too!
    let res = core.mode.control_with_options(
        control_event,
        target,
        (),
        ModeControlOptions::default(),
        // Performance control not relevant in virtual context.
        None,
    )?;
    let transformed_control_value: Option<ControlValue> = res.into();
    let transformed_control_value = transformed_control_value?;
    core.time_of_last_control = Some(Instant::now());
    let res = VirtualSourceValue::new(target.control_element(), transformed_control_value);
    Some(res)
}

#[derive(PartialEq, Debug)]
pub(crate) enum ControlMode {
    Disabled,
    Controlling,
    LearningSource {
        /// Just passed through
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    },
}

/// Supposed to be used to aggregate values of all resolved targets of one mapping into one single
/// value. At the moment we just take the maximum.
pub fn aggregate_target_values(
    values: impl Iterator<Item = Option<AbsoluteValue>>,
) -> Option<AbsoluteValue> {
    values.flatten().max()
}

#[derive(Default)]
pub struct MappingControlResult {
    /// `true` if target hit or almost hit but left untouched because it already has desired value.
    /// `false` e.g. if source message filtered out (e.g. because of button filter) or no target.
    pub at_least_one_target_was_reached: bool,
    /// `true` if at least one target has been invoked already *with an effect*.
    /// Can only be `true` if at least one target has been reached is also `true`.
    pub at_least_one_target_caused_effect: bool,
    /// In case the target doesn't support automatic feedback (even polling not enabled for it),
    /// this should contain the target value determined at the occasion of hitting the target.
    pub new_target_value: Option<AbsoluteValue>,
    /// Even if not hit, this can contain a feedback value (if "Send feedback after control" on)!
    pub feedback_value: Option<CompoundFeedbackValue>,
    pub hit_instruction: Option<BoxedHitInstruction>,
    pub celebrate_success: bool,
}

/// Not usable for mappings with virtual targets.
fn should_send_manual_feedback_due_to_target(
    target: &ReaperTarget,
    options: &ProcessorMappingOptions,
    activation_state: &ActivationState,
    unresolved_target: Option<&UnresolvedCompoundMappingTarget>,
) -> bool {
    if target.supports_automatic_feedback() {
        // The target value was changed and that triggered feedback. Therefore we don't
        // need to send it here a second time (even if `send_feedback_after_control` is
        // enabled). This happens in the majority of cases.
        false
    } else {
        // The target value was changed but the target doesn't support feedback. What a virtual
        // control mapping says shouldn't be relevant here because this is about the target
        // supporting feedback, not about the controller needing the "Send feedback after control"
        // workaround. Therefore we don't forward any "enforce" options.
        feedback_is_effectively_on(options, activation_state, unresolved_target)
    }
}

fn feedback_is_effectively_on(
    options: &ProcessorMappingOptions,
    activation_state: &ActivationState,
    unresolved_target: Option<&UnresolvedCompoundMappingTarget>,
) -> bool {
    is_effectively_active(options, activation_state, unresolved_target)
        && options.feedback_is_effectively_enabled()
}

/// Returns `true` if the mapping itself and the target is active.
fn is_effectively_active(
    options: &ProcessorMappingOptions,
    activation_state: &ActivationState,
    unresolved_target: Option<&UnresolvedCompoundMappingTarget>,
) -> bool {
    activation_state.is_active() && target_is_effectively_active(options, unresolved_target)
}

fn target_is_effectively_active(
    options: &ProcessorMappingOptions,
    unresolved_target: Option<&UnresolvedCompoundMappingTarget>,
) -> bool {
    if options.target_is_active {
        return true;
    }
    if let Some(UnresolvedCompoundMappingTarget::Reaper(t)) = unresolved_target {
        t.is_always_active()
    } else {
        false
    }
}

pub type OrderedMappingMap<T> = IndexMap<MappingId, T>;
pub type OrderedMappingIdSet = IndexSet<MappingId>;

#[derive(Clone, PartialEq, Debug)]
pub enum MessageCaptureResult {
    Midi(MidiScanResult),
    Osc(OscScanResult),
    Keyboard(KeyMessage),
    RealearnParameter(RealearnParameterChangePayload),
}

impl MessageCaptureResult {
    /// Returns the captured source value.
    ///
    /// Used for source filtering, source virtualization, learning (ignoring sources).
    pub fn message(&self) -> IncomingCompoundSourceValue {
        use MessageCaptureResult::*;
        match self {
            Midi(res) => IncomingCompoundSourceValue::Midi(&res.value),
            Osc(res) => IncomingCompoundSourceValue::Osc(&res.message),
            Keyboard(res) => IncomingCompoundSourceValue::Key(*res),
            RealearnParameter(payload) => IncomingCompoundSourceValue::RealearnParameter(*payload),
        }
    }

    /// For finding sessions matching a certain source.
    pub fn to_input_descriptor(&self, ignore_midi_channel: bool) -> Option<InputDescriptor> {
        use MessageCaptureResult::*;
        let res = match self {
            Midi(r) => InputDescriptor::Midi {
                device_id: r.dev_id?,
                channel: if ignore_midi_channel {
                    None
                } else {
                    r.value.channel()
                },
            },
            Osc(r) => InputDescriptor::Osc {
                device_id: r.dev_id?,
            },
            Keyboard(_) => InputDescriptor::Keyboard,
            RealearnParameter(_) => return None,
        };
        Some(res)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum IncomingCompoundSourceValue<'a> {
    Midi(&'a MidiSourceValue<'a, RawShortMessage>),
    Osc(&'a OscMessage),
    Virtual(&'a VirtualSourceValue),
    Key(KeyMessage),
    RealearnParameter(RealearnParameterChangePayload),
}

pub enum InputDescriptor {
    Midi {
        device_id: MidiInputDeviceId,
        channel: Option<Channel>,
    },
    Osc {
        device_id: OscDeviceId,
    },
    Keyboard,
}

#[derive(Copy, Clone)]
pub enum ControlOutcome<T> {
    Consumed,
    Matched(T),
}

#[derive(Eq, PartialEq, derive_more::Display)]
pub enum ControlLogContext {
    #[display(fmt = "normal control")]
    Normal,
    #[display(fmt = "polling")]
    Polling,
    #[display(fmt = "group navigation")]
    GroupNavigation,
    #[display(fmt = "real-time control")]
    RealTime,
    #[display(fmt = "direct control")]
    Direct,
    #[display(fmt = "group interaction")]
    GroupInteraction,
    #[display(fmt = "loading mapping snapshot")]
    LoadingMappingSnapshot,
}

#[derive(Copy, Clone)]
pub struct ControlLogEntry {
    pub kind: ControlLogEntryKind,
    pub control_value: Option<ControlValue>,
    pub error: &'static str,
}

impl Display for ControlLogEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)?;
        if let Some(v) = self.control_value {
            write!(f, " with control value {}", v)?;
        }
        if !self.error.is_empty() {
            write!(f, ": {}", self.error)?;
        }
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Copy, Clone, Eq, PartialEq, derive_more::Display)]
pub enum ControlLogEntryKind {
    /// Event didn't even reach the target because it was filtered out by the glue section.
    FilteredOutByGlue,
    /// Didn't even invoke target because it already has the desired value.
    LeftTargetUntouched,
    /// Target chose to ignore the incoming control value.
    Ignored,
    /// Target executed successfully.
    HitSuccessfully,
    /// Target failed executing.
    HitFailed,
    /// Target created a hit instruction (to be executed later).
    CreatedHitInstruction,
    /// Target created a hit instruction but it was discarded because there was another target
    /// in that mapping whose hit instruction "won" (at the moment, we support only one hit
    /// instruction for multi-targets).
    DiscardedHitInstruction,
    /// Hit instruction was executed successfully.
    ExecutedHitInstructionSuccessfully,
    /// Hit instruction execution failed.
    FailedExecutingHitInstruction,
}
