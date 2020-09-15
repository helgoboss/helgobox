use crate::domain::{
    ActivationCondition, ControlOptions, MainProcessorTargetUpdate, Mode, RealearnTarget,
    ReaperTarget, TargetCharacter, VirtualControlElement, VirtualSource, VirtualSourceValue,
    VirtualTarget,
};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use enum_map::Enum;
use helgoboss_learn::{
    ControlType, ControlValue, MidiSource, MidiSourceValue, SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{RawShortMessage, ShortMessage};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct ProcessorMappingOptions {
    pub mapping_is_active: bool,
    pub target_is_active: bool,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub prevent_echo_feedback: bool,
    pub send_feedback_after_control: bool,
}

impl ProcessorMappingOptions {
    fn control_is_effectively_on(&self) -> bool {
        self.is_active() && self.control_is_enabled
    }

    fn feedback_is_effectively_on(&self) -> bool {
        self.is_active() && self.feedback_is_enabled
    }

    fn is_active(&self) -> bool {
        self.mapping_is_active && self.target_is_active
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
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

const MAX_ECHO_FEEDBACK_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct MainMapping {
    core: MappingCore,
    activation_condition: ActivationCondition,
}

impl MainMapping {
    pub fn id(&self) -> MappingId {
        self.core.id
    }

    /// Returns `Some` if this affects the mapping's activation state and if the resulting state
    /// is on or off.
    pub fn notify_param_changed(
        &self,
        params: &[f32],
        index: u32,
        previous_value: f32,
        value: f32,
    ) -> Option<bool> {
        if self
            .activation_condition
            .notify_param_changed(index, previous_value, value)
        {
            let is_fulfilled = self.activation_condition.is_fulfilled(params);
            Some(is_fulfilled)
        } else {
            None
        }
    }

    pub fn update_activation(&mut self, is_active: bool) {
        self.core.options.mapping_is_active = is_active;
    }

    pub fn update_target(&mut self, update: MainProcessorTargetUpdate) {
        self.core.target = update.target;
        self.core.options.target_is_active = update.target_is_active;
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.core.options.control_is_effectively_on()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.core.options.feedback_is_effectively_on()
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
    ) -> Option<CompoundMappingSourceValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        if let Some(final_value) = self.core.mode.control(value, target) {
            if self.core.options.prevent_echo_feedback || options.enforce_prevent_echo_feedback {
                self.core.time_of_last_control = Some(Instant::now());
            }
            target.control(final_value).unwrap();
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

    pub fn feedback_if_enabled(&self) -> Option<CompoundMappingSourceValue> {
        if !self.feedback_is_effectively_on() {
            return None;
        }
        if let Some(t) = self.core.time_of_last_control {
            if t.elapsed() <= MAX_ECHO_FEEDBACK_DELAY {
                return None;
            }
        }
        let target = match &self.core.target {
            Some(CompoundMappingTarget::Reaper(t)) => t,
            _ => return None,
        };
        let target_value = target.current_value()?;
        let modified_value = self.core.mode.feedback(target_value)?;
        self.core.source.feedback(modified_value)
    }

    fn feedback_after_control_if_enabled(
        &self,
        options: ControlOptions,
    ) -> Option<CompoundMappingSourceValue> {
        if self.core.options.send_feedback_after_control
            || options.enforce_send_feedback_after_control
        {
            self.feedback_if_enabled()
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct RealTimeMapping {
    core: MappingCore,
}

impl RealTimeMapping {
    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.core.options.control_is_effectively_on()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.core.options.feedback_is_effectively_on()
    }

    pub fn update_target_activation(&mut self, is_active: bool) {
        self.core.options.target_is_active = is_active;
    }

    pub fn update_activation(&mut self, is_active: bool) {
        self.core.options.mapping_is_active = is_active;
    }

    pub fn source(&self) -> &CompoundMappingSource {
        &self.core.source
    }

    pub fn mode(&mut self) -> &Mode {
        &self.core.mode
    }

    pub fn target(&self) -> Option<&CompoundMappingTarget> {
        self.core.target.as_ref()
    }

    pub fn consumes(&self, msg: RawShortMessage) -> bool {
        self.core.source.consumes(&msg)
    }

    pub fn options(&self) -> &ProcessorMappingOptions {
        &self.core.options
    }

    pub fn control(
        &mut self,
        source_value: MidiSourceValue<RawShortMessage>,
    ) -> Option<PartialControlMatch> {
        let target = self.core.target.as_ref()?;
        let control_value = self
            .core
            .source
            .control(&CompoundMappingSourceValue::Midi(source_value))?;
        use CompoundMappingTarget::*;
        let result = match target {
            Reaper(_) => {
                // Send to main processor because this needs to be done in main thread.
                PartialControlMatch::ForwardToMain(control_value)
            }
            Virtual(t) => {
                // Determine resulting virtual control value in real-time processor.
                // It's important to do that here. We need to know the result in order to
                // return if there was actually a match of *real* non-virtual mappings.
                // Unlike with REAPER targets, we also don't have threading issues here :)
                let transformed_control_value = self.core.mode.control(control_value, t)?;
                PartialControlMatch::ProcessVirtual(VirtualSourceValue::new(
                    t.control_element(),
                    transformed_control_value,
                ))
            }
        };
        Some(result)
    }

    pub fn feedback(&self, feedback_value: UnitValue) -> Option<CompoundMappingSourceValue> {
        let transformed_feedback_value = self.core.mode.feedback(feedback_value)?;
        self.core.source.feedback(transformed_feedback_value)
    }
}

pub enum PartialControlMatch {
    ProcessVirtual(VirtualSourceValue),
    ForwardToMain(ControlValue),
}

#[derive(Clone, Debug)]
pub struct MappingCore {
    id: MappingId,
    source: CompoundMappingSource,
    mode: Mode,
    target: Option<CompoundMappingTarget>,
    options: ProcessorMappingOptions,
    time_of_last_control: Option<Instant>,
}

#[derive(Debug)]
pub struct Mapping {
    core: MappingCore,
    activation_condition: ActivationCondition,
}

impl Mapping {
    pub fn new(
        id: MappingId,
        source: CompoundMappingSource,
        mode: Mode,
        target: Option<CompoundMappingTarget>,
        activation_condition: ActivationCondition,
        options: ProcessorMappingOptions,
    ) -> Mapping {
        Mapping {
            core: MappingCore {
                id,
                source,
                mode,
                target,
                options,
                time_of_last_control: None,
            },
            activation_condition,
        }
    }

    pub fn id(&self) -> MappingId {
        self.core.id
    }

    pub fn splinter(self) -> (RealTimeMapping, MainMapping) {
        let real_time_mapping = RealTimeMapping {
            core: self.core.clone(),
        };
        let main_mapping = MainMapping {
            core: self.core,
            activation_condition: self.activation_condition,
        };
        (real_time_mapping, main_mapping)
    }

    pub fn into_main_processor_target_update(self) -> MainProcessorTargetUpdate {
        MainProcessorTargetUpdate {
            id: self.core.id,
            target: self.core.target,
            target_is_active: self.core.options.target_is_active,
        }
    }

    pub fn target_is_active(&self) -> bool {
        self.core.options.target_is_active
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub enum CompoundMappingSource {
    Midi(MidiSource),
    Virtual(VirtualSource),
}

impl CompoundMappingSource {
    pub fn control(&self, value: &CompoundMappingSourceValue) -> Option<ControlValue> {
        use CompoundMappingSource::*;
        match (self, value) {
            (Midi(s), CompoundMappingSourceValue::Midi(v)) => s.control(v),
            (Virtual(s), CompoundMappingSourceValue::Virtual(v)) => s.control(v),
            _ => None,
        }
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.format_control_value(value),
            Virtual(s) => s.format_control_value(value),
        }
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.parse_control_value(text),
            Virtual(s) => s.parse_control_value(text),
        }
    }

    pub fn character(&self) -> ExtendedSourceCharacter {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => ExtendedSourceCharacter::Normal(s.character()),
            Virtual(s) => s.character(),
        }
    }

    pub fn feedback(&self, feedback_value: UnitValue) -> Option<CompoundMappingSourceValue> {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s
                .feedback(feedback_value)
                .map(CompoundMappingSourceValue::Midi),
            Virtual(s) => Some(CompoundMappingSourceValue::Virtual(
                s.feedback(feedback_value),
            )),
        }
    }

    pub fn consumes(&self, msg: &impl ShortMessage) -> bool {
        use CompoundMappingSource::*;
        match self {
            Midi(s) => s.consumes(msg),
            Virtual(_) => false,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CompoundMappingSourceValue {
    Midi(MidiSourceValue<RawShortMessage>),
    Virtual(VirtualSourceValue),
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
    #[display(fmt = "Controller mappings")]
    ControllerMappings,
    #[display(fmt = "Primary mappings")]
    PrimaryMappings,
}

pub enum ExtendedSourceCharacter {
    Normal(SourceCharacter),
    VirtualContinuous,
}
