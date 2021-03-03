use crate::domain::{
    ActivationChange, ActivationCondition, ControlOptions, MappingActivationEffect, Mode,
    ParameterArray, ProcessorContext, RealearnTarget, ReaperTarget, TargetCharacter,
    UnresolvedReaperTarget, VirtualSource, VirtualSourceValue, VirtualTarget,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use helgoboss_learn::{
    ControlType, ControlValue, MidiSource, MidiSourceValue, OscSource, SourceCharacter, Target,
    UnitValue,
};
use helgoboss_midi::{RawShortMessage, ShortMessage};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use reaper_low::raw;
use reaper_medium::MidiFrameOffset;
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
    Raw(Box<RawMidiData>),
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
    activation_condition_1: ActivationCondition,
    activation_condition_2: ActivationCondition,
    is_active_1: bool,
    is_active_2: bool,
    extension: MappingExtension,
}

impl MainMapping {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
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
                id,
                source,
                mode,
                unresolved_target,
                target: None,
                options,
                time_of_last_control: None,
            },
            activation_condition_1,
            activation_condition_2,
            is_active_1: false,
            is_active_2: false,
            extension,
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

    /// Returns if this activation condition is affected by parameter changes in general.
    pub fn can_be_affected_by_parameters(&self) -> bool {
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

    pub fn refresh_all(&mut self, context: &ProcessorContext, params: &ParameterArray) {
        self.refresh_target(context);
        self.update_activation(params);
    }

    pub fn needs_refresh_when_target_touched(&self) -> bool {
        matches!(
            self.core.unresolved_target,
            Some(UnresolvedCompoundMappingTarget::Reaper(
                UnresolvedReaperTarget::LastTouched
            ))
        )
    }

    pub fn refresh_target(&mut self, context: &ProcessorContext) -> Option<ActivationChange> {
        let was_active_before = self.core.options.target_is_active;
        let (target, is_active) = match self.core.unresolved_target.as_ref() {
            None => (None, false),
            Some(t) => match t.resolve(context).ok() {
                None => (None, false),
                Some(rt) => {
                    let met = t.conditions_are_met(&rt);
                    (Some(rt), met)
                }
            },
        };
        self.core.target = target;
        self.core.options.target_is_active = is_active;
        if is_active == was_active_before {
            return None;
        }
        let update = ActivationChange {
            id: self.id(),
            is_active,
        };
        Some(update)
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
        self.has_virtual_target() || (self.is_active_1 && self.is_active_2)
    }

    fn is_effectively_active(&self) -> bool {
        self.has_virtual_target() || (self.is_active() && self.core.options.target_is_active)
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

    /// Controls mode => target.
    ///
    /// Don't execute in real-time processor because this executes REAPER main-thread-only
    /// functions. If `send_feedback_after_control` is on, this might return feedback.
    pub fn control_if_enabled(
        &mut self,
        value: ControlValue,
        options: ControlOptions,
    ) -> Option<SourceValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        if let Some(final_value) = self.core.mode.control(value, target) {
            if self.core.options.prevent_echo_feedback {
                self.core.time_of_last_control = Some(Instant::now());
            }
            // Be graceful here. TODO-medium In future we could display some kind of small
            // unintrusive error message somewhere.
            let _ = target.control(final_value);
            if target.supports_feedback() {
                // The target value was changed and that triggered feedback. Therefore we don't
                // need to send it here a second time (even if `send_feedback_after_control` is
                // enabled). This happens in the majority of cases.
                None
            } else {
                // The target value was changed but the target doesn't support feedback. If
                // `send_feedback_after_control` is enabled, we at least send feedback after we
                // know it has been changed.
                self.feedback_after_control_if_enabled(options)
            }
        } else {
            // The target value was not changed. If `send_feedback_after_control` is enabled, we
            // still send feedback - this can be useful with controllers which insist controlling
            // the LED on their own. The feedback sent by ReaLearn will fix this self-controlled
            // LED state.
            self.feedback_after_control_if_enabled(options)
        }
    }

    pub fn feedback_if_enabled(&self) -> Option<SourceValue> {
        if !self.feedback_is_effectively_on() {
            return None;
        }
        self.feedback()
    }

    pub fn feedback(&self) -> Option<SourceValue> {
        if self.core.is_echo() {
            return None;
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        let target_value = target.current_value()?;
        self.feedback_given_value(target_value)
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

    pub fn feedback_given_value(&self, value: UnitValue) -> Option<SourceValue> {
        let modified_value = self.core.mode.feedback(value)?;
        self.core.source.feedback(modified_value)
    }

    pub fn zero_feedback(&self) -> Option<SourceValue> {
        self.source().feedback(UnitValue::MIN)
    }

    fn feedback_after_control_if_enabled(&self, options: ControlOptions) -> Option<SourceValue> {
        if self.core.options.send_feedback_after_control
            || options.enforce_send_feedback_after_control
        {
            self.feedback_if_enabled()
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

/// Raw MIDI data which is compatible to both VST and REAPER MIDI data structures. The REAPER
/// struct is more picky in that it needs offset and size directly in front of the raw data whereas
/// the VST struct allows the data to be at a different address. That's why we need to follow the
/// REAPER requirement.
#[derive(Debug)]
pub struct RawMidiData {
    midi_event: OwnedMidiEvent,
}

impl RawMidiData {
    pub fn try_from_slice(midi_message: &[u8]) -> Result<Self, &'static str> {
        let evt = OwnedMidiEvent::try_from_slice(MidiFrameOffset::new(0), midi_message)?;
        Ok(Self::new(evt))
    }

    fn new(midi_event: OwnedMidiEvent) -> Self {
        Self { midi_event }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.midi_event.bytes()
    }
}

impl AsRef<raw::MIDI_event_t> for RawMidiData {
    fn as_ref(&self) -> &raw::MIDI_event_t {
        self.midi_event.as_ref()
    }
}

const MAX_RAW_MIDI_DATA_LENGTH: usize = 256;

/// An owned REAPER MIDI message.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(C)]
pub struct OwnedMidiEvent {
    frame_offset: i32,
    size: i32,
    midi_message: [u8; MAX_RAW_MIDI_DATA_LENGTH],
}

impl OwnedMidiEvent {
    pub fn new(
        frame_offset: MidiFrameOffset,
        size: u32,
        midi_message: [u8; MAX_RAW_MIDI_DATA_LENGTH],
    ) -> Self {
        Self {
            frame_offset: frame_offset.to_raw(),
            size: size as _,
            midi_message,
        }
    }

    pub fn try_from_slice(
        frame_offset: MidiFrameOffset,
        midi_message: &[u8],
    ) -> Result<Self, &'static str> {
        if midi_message.len() > MAX_RAW_MIDI_DATA_LENGTH {
            return Err("given MIDI message too long");
        }
        let mut array = [0; MAX_RAW_MIDI_DATA_LENGTH];
        // TODO-low I think copying from a slice is the only way to go. If we have an existing vec,
        //  then REAPER's struct layout requires us to put something in front of the vec, which is
        //  not easily possible without copying.
        array[..midi_message.len()].copy_from_slice(&midi_message);
        Ok(Self::new(frame_offset, midi_message.len() as _, array))
    }

    fn bytes(&self) -> &[u8] {
        &self.midi_message[..self.size as usize]
    }
}

impl AsRef<raw::MIDI_event_t> for OwnedMidiEvent {
    fn as_ref(&self) -> &raw::MIDI_event_t {
        unsafe { &*(self as *const OwnedMidiEvent as *const raw::MIDI_event_t) }
    }
}

#[derive(Debug)]
pub struct RealTimeMapping {
    core: MappingCore,
    is_active: bool,
    lifecycle_midi_data: LifecycleMidiData,
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
        self.has_virtual_target() || (self.is_active && self.core.options.target_is_active)
    }

    fn is_effectively_active_ignoring_target_activation(&self) -> bool {
        self.has_virtual_target() || self.is_active
    }

    fn is_effectively_active_ignoring_mapping_activation(&self) -> bool {
        self.has_virtual_target() || self.core.options.target_is_active
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

    pub fn target(&self) -> Option<&UnresolvedCompoundMappingTarget> {
        self.core.unresolved_target.as_ref()
    }

    pub fn has_virtual_target(&self) -> bool {
        matches!(
            self.target(),
            Some(UnresolvedCompoundMappingTarget::Virtual(_))
        )
    }

    pub fn has_reaper_target(&self) -> bool {
        matches!(
            self.core.unresolved_target,
            Some(UnresolvedCompoundMappingTarget::Reaper(_))
        )
    }

    pub fn consumes(&self, msg: RawShortMessage) -> bool {
        self.core.source.consumes(&msg)
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn control_midi_virtualizing(
        &mut self,
        source_value: MidiSourceValue<RawShortMessage>,
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
    id: MappingId,
    source: CompoundMappingSource,
    mode: Mode,
    // TODO-medium Take targets out of MappingCore because RealTimeMapping doesn't need most of it!
    unresolved_target: Option<UnresolvedCompoundMappingTarget>,
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

    pub fn feedback(&self, feedback_value: UnitValue) -> Option<SourceValue> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.feedback(feedback_value).map(SourceValue::Midi),
            Virtual(s) => Some(SourceValue::Virtual(s.feedback(feedback_value))),
            Osc(s) => s.feedback(feedback_value).map(SourceValue::Osc),
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
pub enum SourceValue {
    Midi(MidiSourceValue<RawShortMessage>),
    Virtual(VirtualSourceValue),
    Osc(OscMessage),
}

#[derive(Clone, PartialEq, Debug)]
pub enum UnresolvedCompoundMappingTarget {
    Reaper(UnresolvedReaperTarget),
    Virtual(VirtualTarget),
}

impl UnresolvedCompoundMappingTarget {
    pub fn resolve(
        &self,
        context: &ProcessorContext,
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
