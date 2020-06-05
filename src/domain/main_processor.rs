use crate::domain::{MainProcessorMapping, MappingId, Mode, ReaperTarget};
use helgoboss_learn::{ControlValue, MidiSource};

const BULK_SIZE: usize = 30;

#[derive(Debug)]
pub struct MainProcessor {
    mappings: Vec<MainProcessorMapping>,
    receiver: crossbeam_channel::Receiver<MainProcessorTask>,
}

impl MainProcessor {
    pub fn new(receiver: crossbeam_channel::Receiver<MainProcessorTask>) -> MainProcessor {
        MainProcessor {
            receiver,
            mappings: vec![],
        }
    }

    pub fn update_mappings(&mut self, mappings: Vec<MainProcessorMapping>) {
        self.mappings = mappings;
    }

    /// Should be called regularly in main thread.
    pub fn idle(&self) {
        for task in self.receiver.try_iter().take(BULK_SIZE) {
            use MainProcessorTask::*;
            match task {
                Control { mapping_id, value } => {
                    self.process(mapping_id, value);
                }
                LearnSource(source) => todo!(),
            }
        }
    }

    fn process(&self, mapping_id: MappingId, value: ControlValue) {
        let mapping = match self.mappings.get(mapping_id.index() as usize) {
            None => return,
            Some(m) => m,
        };
        mapping.control(value);
    }
}

#[derive(Debug)]
pub enum MainProcessorTask {
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}
