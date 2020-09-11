use crate::domain::{
    ActivationCondition, MainProcessorTargetUpdate, Mode, ReaperTarget, VirtualControlElement,
    VirtualSource, VirtualSourceValue, VirtualTarget,
};
use helgoboss_learn::{
    ControlValue, MidiSource, MidiSourceValue, SourceCharacter, Target, UnitValue,
};
use helgoboss_midi::{RawShortMessage, ShortMessage};

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

#[derive(Debug)]
pub struct NormalMapping {
    id: MappingId,
    source: NormalMappingSource,
    mode: Mode,
    target: Option<ReaperTarget>,
    activation_condition: ActivationCondition,
    options: ProcessorMappingOptions,
}

impl NormalMapping {
    pub fn new(
        id: MappingId,
        source: NormalMappingSource,
        mode: Mode,
        target: Option<ReaperTarget>,
        activation_condition: ActivationCondition,
        options: ProcessorMappingOptions,
    ) -> NormalMapping {
        NormalMapping {
            id,
            source,
            mode,
            target,
            activation_condition,
            options,
        }
    }

    pub fn splinter(self) -> (NormalRealTimeMapping, NormalMainMapping) {
        let real_time_mapping =
            NormalRealTimeMapping::new(self.id, self.source.clone(), self.options);
        let main_mapping = NormalMainMapping::new(
            self.id,
            self.source.clone(),
            self.mode.clone(),
            self.target.clone(),
            self.activation_condition,
            self.options,
        );
        (real_time_mapping, main_mapping)
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

#[derive(Debug)]
pub struct NormalRealTimeMapping {
    id: MappingId,
    source: NormalMappingSource,
    options: ProcessorMappingOptions,
}

impl NormalRealTimeMapping {
    pub fn new(
        id: MappingId,
        source: NormalMappingSource,
        options: ProcessorMappingOptions,
    ) -> NormalRealTimeMapping {
        NormalRealTimeMapping {
            source,
            id,
            options,
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn control(&self, value: &NormalMappingSourceValue) -> Option<ControlValue> {
        self.source.control(value)
    }

    pub fn consumes(&self, msg: RawShortMessage) -> bool {
        self.source.consumes(&msg)
    }

    pub fn target_is_active(&self) -> bool {
        self.options.target_is_active
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.options.control_is_effectively_on()
    }

    pub fn update_target_activation(&mut self, is_active: bool) {
        self.options.target_is_active = is_active;
    }

    pub fn update_mapping_activation(&mut self, is_active: bool) {
        self.options.mapping_is_active = is_active;
    }
}

const MAX_ECHO_FEEDBACK_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct NormalMainMapping {
    id: MappingId,
    source: NormalMappingSource,
    mode: Mode,
    target: Option<ReaperTarget>,
    activation_condition: ActivationCondition,
    options: ProcessorMappingOptions,
    time_of_last_control: Option<Instant>,
}

impl NormalMainMapping {
    pub fn new(
        id: MappingId,
        source: NormalMappingSource,
        mode: Mode,
        target: Option<ReaperTarget>,
        activation_condition: ActivationCondition,
        options: ProcessorMappingOptions,
    ) -> NormalMainMapping {
        NormalMainMapping {
            id,
            source,
            mode,
            target,
            activation_condition,
            options,
            time_of_last_control: None,
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn update_target(&mut self, update: MainProcessorTargetUpdate) {
        self.target = update.target;
        self.options.target_is_active = update.target_is_active;
    }

    pub fn update_activation(&mut self, is_active: bool) {
        self.options.mapping_is_active = is_active;
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

    pub fn into_main_processor_target_update(self) -> MainProcessorTargetUpdate {
        MainProcessorTargetUpdate {
            id: self.id(),
            target: self.target,
            target_is_active: self.options.target_is_active,
        }
    }

    /// If `send_feedback_after_control` is on, this might return feedback.
    pub fn control_if_enabled(&mut self, value: ControlValue) -> Option<NormalMappingSourceValue> {
        if !self.control_is_effectively_on() {
            return None;
        }
        let target = match &self.target {
            None => return None,
            Some(t) => t,
        };
        if let Some(final_value) = self.mode.control(value, target) {
            if self.options.prevent_echo_feedback {
                self.time_of_last_control = Some(Instant::now());
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
                self.feedback_after_control_if_enabled()
            }
        } else {
            // The target value was not changed. If `send_feedback_after_control` is enabled, we
            // still send feedback - this can be useful with controllers which insist controlling
            // the LED on their own. The feedback sent by ReaLearn will fix this self-controlled
            // LED state.
            self.feedback_after_control_if_enabled()
        }
    }

    pub fn feedback_if_enabled(&self) -> Option<NormalMappingSourceValue> {
        if !self.feedback_is_effectively_on() {
            return None;
        }
        if let Some(t) = self.time_of_last_control {
            if t.elapsed() <= MAX_ECHO_FEEDBACK_DELAY {
                return None;
            }
        }
        let target = match &self.target {
            None => return None,
            Some(t) => t,
        };
        let target_value = target.current_value();
        let modified_value = self.mode.feedback(target_value)?;
        self.source.feedback(modified_value)
    }

    pub fn source(&self) -> &NormalMappingSource {
        &self.source
    }

    pub fn target(&self) -> Option<&ReaperTarget> {
        self.target.as_ref()
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.options.control_is_effectively_on()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.options.feedback_is_effectively_on()
    }

    fn feedback_after_control_if_enabled(&self) -> Option<NormalMappingSourceValue> {
        if self.options.send_feedback_after_control {
            self.feedback_if_enabled()
        } else {
            None
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
pub enum NormalMappingSource {
    Midi(MidiSource),
    Virtual(VirtualSource),
}

impl NormalMappingSource {
    pub fn control(&self, value: &NormalMappingSourceValue) -> Option<ControlValue> {
        use NormalMappingSource::*;
        match (self, value) {
            (Midi(s), NormalMappingSourceValue::Midi(v)) => s.control(v),
            (Virtual(s), NormalMappingSourceValue::Virtual(v)) => s.control(v),
            _ => None,
        }
    }

    pub fn format_control_value(&self, value: ControlValue) -> Result<String, &'static str> {
        use NormalMappingSource::*;
        match self {
            Midi(s) => s.format_control_value(value),
            Virtual(s) => s.format_control_value(value),
        }
    }

    pub fn parse_control_value(&self, text: &str) -> Result<UnitValue, &'static str> {
        use NormalMappingSource::*;
        match self {
            Midi(s) => s.parse_control_value(text),
            Virtual(s) => s.parse_control_value(text),
        }
    }

    pub fn character(&self) -> SourceCharacter {
        use NormalMappingSource::*;
        match self {
            Midi(s) => s.character(),
            Virtual(s) => s.character(),
        }
    }

    pub fn feedback(&self, feedback_value: UnitValue) -> Option<NormalMappingSourceValue> {
        use NormalMappingSource::*;
        match self {
            Midi(s) => s
                .feedback(feedback_value)
                .map(NormalMappingSourceValue::Midi),
            Virtual(s) => Some(NormalMappingSourceValue::Virtual(
                s.feedback(feedback_value),
            )),
        }
    }

    pub fn consumes(&self, msg: &impl ShortMessage) -> bool {
        use NormalMappingSource::*;
        match self {
            Midi(s) => s.consumes(msg),
            Virtual(_) => false,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum NormalMappingSourceValue {
    Midi(MidiSourceValue<RawShortMessage>),
    Virtual(VirtualSourceValue),
}

#[derive(Debug)]
pub struct ControllerMapping {
    source: MidiSource,
    mode: Mode,
    target: Option<ControllerMappingTarget>,
    options: ProcessorMappingOptions,
}

#[derive(Debug)]
pub struct VirtualMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: VirtualTarget,
    options: ProcessorMappingOptions,
}

impl VirtualMapping {
    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn control_is_effectively_on(&self) -> bool {
        self.options.control_is_effectively_on()
    }

    pub fn feedback_is_effectively_on(&self) -> bool {
        self.options.feedback_is_effectively_on()
    }

    pub fn control(&self, value: &MidiSourceValue<RawShortMessage>) -> Option<VirtualSourceValue> {
        let control_value = self.source.control(value)?;
        Some(VirtualSourceValue::new(
            self.target.control_element(),
            control_value,
        ))
    }

    pub fn feedback(
        &self,
        control_element: VirtualControlElement,
        value: UnitValue,
    ) -> Option<MidiSourceValue<RawShortMessage>> {
        if self.target.control_element() != control_element {
            return None;
        }
        self.source.feedback(value)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ControllerMappingTarget {
    Reaper(ReaperTarget),
    Virtual(VirtualTarget),
}
