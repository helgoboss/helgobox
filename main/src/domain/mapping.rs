use crate::domain::{MainProcessorTargetUpdate, Mode, ReaperTarget};
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, Target};
use helgoboss_midi::RawShortMessage;

use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Copy, Clone, Debug)]
pub struct ProcessorMappingOptions {
    pub mapping_is_active: bool,
    pub target_is_active: bool,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
    pub prevent_echo_feedback: bool,
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
pub struct ProcessorMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: Option<ReaperTarget>,
    options: ProcessorMappingOptions,
}

impl ProcessorMapping {
    pub fn new(
        id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: Option<ReaperTarget>,
        options: ProcessorMappingOptions,
    ) -> ProcessorMapping {
        ProcessorMapping {
            id,
            source,
            mode,
            target,
            options,
        }
    }

    pub fn splinter(&self) -> (RealTimeProcessorMapping, MainProcessorMapping) {
        let real_time_mapping =
            RealTimeProcessorMapping::new(self.id, self.source.clone(), self.options);
        let main_mapping = MainProcessorMapping::new(
            self.id,
            self.source.clone(),
            self.mode.clone(),
            self.target.clone(),
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
pub struct RealTimeProcessorMapping {
    id: MappingId,
    source: MidiSource,
    options: ProcessorMappingOptions,
}

impl RealTimeProcessorMapping {
    pub fn new(
        id: MappingId,
        source: MidiSource,
        options: ProcessorMappingOptions,
    ) -> RealTimeProcessorMapping {
        RealTimeProcessorMapping {
            source,
            id,
            options,
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn control(&self, value: &MidiSourceValue<RawShortMessage>) -> Option<ControlValue> {
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
pub struct MainProcessorMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: Option<ReaperTarget>,
    options: ProcessorMappingOptions,
    time_of_last_control: Option<Instant>,
}

impl MainProcessorMapping {
    // TODO-low Improve this bool hell
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: Option<ReaperTarget>,
        options: ProcessorMappingOptions,
    ) -> MainProcessorMapping {
        MainProcessorMapping {
            id,
            source,
            mode,
            target,
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

    pub fn into_main_processor_target_update(self) -> MainProcessorTargetUpdate {
        MainProcessorTargetUpdate {
            id: self.id(),
            target: self.target,
            target_is_active: self.options.target_is_active,
        }
    }

    pub fn control_if_enabled(&mut self, value: ControlValue) {
        if !self.control_is_effectively_on() {
            return;
        }
        let target = match &self.target {
            None => return,
            Some(t) => t,
        };
        if let Some(final_value) = self.mode.control(value, target) {
            if self.options.prevent_echo_feedback {
                self.time_of_last_control = Some(Instant::now());
            }
            target.control(final_value).unwrap();
        }
    }

    pub fn feedback_if_enabled(&self) -> Option<MidiSourceValue<RawShortMessage>> {
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
        let modified_value = self.mode.feedback(target_value);
        self.source.feedback(modified_value)
    }

    pub fn source(&self) -> &MidiSource {
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
}
