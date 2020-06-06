use crate::domain::{Mode, ReaperTarget};
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, Target};
use helgoboss_midi::RawShortMessage;
use rx_util::BoxedUnitEvent;

#[derive(Debug)]
pub struct ProcessorMapping {
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
}

impl ProcessorMapping {
    pub fn new(
        source: MidiSource,
        mode: Mode,
        target: ReaperTarget,
        control_is_enabled: bool,
        feedback_is_enabled: bool,
    ) -> ProcessorMapping {
        ProcessorMapping {
            source,
            mode,
            target,
            control_is_enabled,
            feedback_is_enabled,
        }
    }

    pub fn for_control(
        &self,
        mapping_id: MappingId,
    ) -> (RealTimeProcessorControlMapping, MainProcessorControlMapping) {
        let real_time_mapping =
            RealTimeProcessorControlMapping::new(mapping_id, self.source.clone());
        let main_mapping =
            MainProcessorControlMapping::new(mapping_id, self.mode.clone(), self.target.clone());
        (real_time_mapping, main_mapping)
    }

    pub fn for_feedback(&self, mapping_id: MappingId) -> MainProcessorFeedbackMapping {
        MainProcessorFeedbackMapping::new(
            mapping_id,
            self.source.clone(),
            self.mode.clone(),
            self.target.clone(),
        )
    }

    pub fn control_is_enabled(&self) -> bool {
        self.control_is_enabled
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingId {
    index: u16,
}

impl MappingId {
    pub fn new(index: u16) -> MappingId {
        MappingId { index }
    }

    pub fn index(self) -> u16 {
        self.index
    }
}

// TODO Maybe make fields private as soon as API clear
#[derive(Debug)]
pub struct RealTimeProcessorControlMapping {
    pub mapping_id: MappingId,
    pub source: MidiSource,
}

impl RealTimeProcessorControlMapping {
    pub fn new(mapping_id: MappingId, source: MidiSource) -> RealTimeProcessorControlMapping {
        RealTimeProcessorControlMapping { source, mapping_id }
    }
}

#[derive(Debug)]
pub struct MainProcessorControlMapping {
    // TODO-medium Not used yet
    mapping_id: MappingId,
    mode: Mode,
    target: ReaperTarget,
}

impl MainProcessorControlMapping {
    pub fn new(
        mapping_id: MappingId,
        mode: Mode,
        target: ReaperTarget,
    ) -> MainProcessorControlMapping {
        MainProcessorControlMapping {
            mapping_id,
            mode,
            target,
        }
    }

    pub fn control(&self, value: ControlValue) {
        if let Some(final_value) = self.mode.control(value, &self.target) {
            self.target.control(final_value);
        }
    }
}

#[derive(Debug)]
pub struct MainProcessorFeedbackMapping {
    mapping_id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
}

impl MainProcessorFeedbackMapping {
    pub fn new(
        mapping_id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: ReaperTarget,
    ) -> MainProcessorFeedbackMapping {
        MainProcessorFeedbackMapping {
            mapping_id,
            source,
            mode,
            target,
        }
    }

    pub fn mapping_id(&self) -> MappingId {
        self.mapping_id
    }

    pub fn target_value_changed(&self) -> BoxedUnitEvent {
        self.target.value_changed()
    }

    pub fn feedback(&self) -> Option<MidiSourceValue<RawShortMessage>> {
        let target_value = self.target.current_value();
        let modified_value = self.mode.feedback(target_value);
        self.source.feedback(modified_value)
    }
}
