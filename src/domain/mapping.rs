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

    pub fn splinter(
        &self,
        feedback_is_globally_enabled: bool,
    ) -> (Option<RealTimeProcessorMapping>, MainProcessorMapping) {
        // Real-time processor gets the mapping only if control is enabled.
        let real_time_mapping = if self.control_is_enabled {
            Some(RealTimeProcessorMapping::new(self.id, self.source.clone()))
        } else {
            None
        };
        // Main processor gets the mapping in any case.
        let main_mapping = MainProcessorMapping::new(
            self.id,
            self.source.clone(),
            self.mode.clone(),
            self.target.clone(),
            self.control_is_enabled,
            self.feedback_is_enabled && feedback_is_globally_enabled,
        );
        (real_time_mapping, main_mapping)
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
pub struct RealTimeProcessorMapping {
    id: MappingId,
    source: MidiSource,
}

impl RealTimeProcessorMapping {
    pub fn new(mapping_id: MappingId, source: MidiSource) -> RealTimeProcessorMapping {
        RealTimeProcessorMapping {
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
pub struct MainProcessorMapping {
    id: MappingId,
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
    control_is_enabled: bool,
    feedback_is_enabled: bool,
}

impl MainProcessorMapping {
    pub fn new(
        id: MappingId,
        source: MidiSource,
        mode: Mode,
        target: ReaperTarget,
        control: bool,
        feedback: bool,
    ) -> MainProcessorMapping {
        MainProcessorMapping {
            id,
            source,
            mode,
            target,
            control_is_enabled: control,
            feedback_is_enabled: feedback,
        }
    }

    pub fn id(&self) -> MappingId {
        self.id
    }

    pub fn control_is_enabled(&self) -> bool {
        self.control_is_enabled
    }

    pub fn feedback_is_enabled(&self) -> bool {
        self.feedback_is_enabled
    }

    pub fn control_if_enabled(&mut self, value: ControlValue) {
        if !self.control_is_enabled {
            return;
        }
        if let Some(final_value) = self.mode.control(value, &self.target) {
            self.target.control(final_value);
        }
    }

    pub fn feedback_if_enabled(&self) -> Option<MidiSourceValue<RawShortMessage>> {
        if !self.feedback_is_enabled {
            return None;
        }
        let target_value = self.target.current_value();
        let modified_value = self.mode.feedback(target_value);
        self.source.feedback(modified_value)
    }

    pub fn target_value_changed(&self) -> BoxedUnitEvent {
        self.target.value_changed()
    }
}
