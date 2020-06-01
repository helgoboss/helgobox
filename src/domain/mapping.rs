use crate::domain::{Mode, ReaperTarget};
use helgoboss_learn::{ControlValue, MidiSource};

#[derive(Debug)]
pub struct ProcessorMapping {
    source: MidiSource,
    mode: Mode,
    target: ReaperTarget,
}

impl ProcessorMapping {
    pub fn new(source: MidiSource, mode: Mode, target: ReaperTarget) -> ProcessorMapping {
        ProcessorMapping {
            source,
            mode,
            target,
        }
    }

    pub fn splinter(
        self,
        mapping_id: MappingId,
    ) -> (RealTimeProcessorMapping, MainProcessorMapping) {
        let real_time_mapping = RealTimeProcessorMapping::new(mapping_id, self.source);
        let main_mapping = MainProcessorMapping::new(mapping_id, self.mode, self.target);
        (real_time_mapping, main_mapping)
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
pub struct RealTimeProcessorMapping {
    pub mapping_id: MappingId,
    pub source: MidiSource,
}

impl RealTimeProcessorMapping {
    pub fn new(mapping_id: MappingId, source: MidiSource) -> RealTimeProcessorMapping {
        RealTimeProcessorMapping { source, mapping_id }
    }
}

#[derive(Debug)]
pub struct MainProcessorMapping {
    mapping_id: MappingId,
    mode: Mode,
    target: ReaperTarget,
}

impl MainProcessorMapping {
    pub fn new(mapping_id: MappingId, mode: Mode, target: ReaperTarget) -> MainProcessorMapping {
        MainProcessorMapping {
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
