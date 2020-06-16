use crate::domain::{Mode, ReaperTarget};
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, Target};
use helgoboss_midi::RawShortMessage;
use rx_util::BoxedUnitEvent;
use uuid::Uuid;

#[derive(Debug)]
pub struct ProcessorMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
}

impl ProcessorMapping {
    pub fn new(
        id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: ReaperTarget,
        control_is_enabled: bool,
        feedback_is_enabled: bool,
    ) -> ProcessorMapping {
        ProcessorMapping {
            id,
            source,
            mode,
            target,
            control_is_enabled,
            feedback_is_enabled,
        }
    }

    pub fn id(&self) -> &MappingId {
        &self.id
    }

    pub fn for_control(
        &self,
    ) -> Option<(RealTimeProcessorControlMapping, MainProcessorControlMapping)> {
        if !self.control_is_enabled {
            return None;
        }
        let real_time_mapping = RealTimeProcessorControlMapping::new(self.id, self.source.clone());
        let main_mapping =
            MainProcessorControlMapping::new(self.id, self.mode.clone(), self.target.clone());
        Some((real_time_mapping, main_mapping))
    }

    pub fn for_feedback(self) -> Option<MainProcessorFeedbackMapping> {
        if !self.feedback_is_enabled {
            return None;
        }
        Some(MainProcessorFeedbackMapping::new(
            self.id,
            self.source,
            self.mode,
            self.target,
        ))
    }

    pub fn control_is_enabled(&self) -> bool {
        self.control_is_enabled
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled
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
pub struct RealTimeProcessorControlMapping {
    id: MappingId,
    source: MidiSource,
}

impl RealTimeProcessorControlMapping {
    pub fn new(mapping_id: MappingId, source: MidiSource) -> RealTimeProcessorControlMapping {
        RealTimeProcessorControlMapping {
            source,
            id: mapping_id,
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
}

#[derive(Debug)]
pub struct MainProcessorControlMapping {
    id: MappingId,
    mode: Mode,
    target: ReaperTarget,
}

impl MainProcessorControlMapping {
    pub fn new(id: MappingId, mode: Mode, target: ReaperTarget) -> MainProcessorControlMapping {
        MainProcessorControlMapping { id, mode, target }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn control(&mut self, value: ControlValue) {
        if let Some(final_value) = self.mode.control(value, &self.target) {
            self.target.control(final_value);
        }
    }
}

#[derive(Debug)]
pub struct MainProcessorFeedbackMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
}

impl MainProcessorFeedbackMapping {
    pub fn new(
        id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: ReaperTarget,
    ) -> MainProcessorFeedbackMapping {
        MainProcessorFeedbackMapping {
            id,
            source,
            mode,
            target,
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
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
