use crate::domain::{MainProcessorMapping, MappingId, Mode, ReaperTarget, SharedSession};
use helgoboss_learn::{ControlValue, MidiSource};
use reaper_medium::ControlSurface;

const BULK_SIZE: usize = 30;

#[derive(Debug)]
pub struct MainProcessor {
    mappings: Vec<MainProcessorMapping>,
    receiver: crossbeam_channel::Receiver<MainProcessorTask>,
    session: SharedSession,
}

impl ControlSurface for MainProcessor {
    fn run(&mut self) {
        for task in self.receiver.try_iter().take(BULK_SIZE) {
            use MainProcessorTask::*;
            match task {
                UpdateMappings(mappings) => {
                    self.mappings = mappings;
                }
                Control { mapping_id, value } => {
                    self.control(mapping_id, value);
                }
                LearnSource(source) => {
                    self.session.borrow_mut().learn_source(&source);
                }
            }
        }
    }
}

impl MainProcessor {
    pub fn new(
        receiver: crossbeam_channel::Receiver<MainProcessorTask>,
        session: SharedSession,
    ) -> MainProcessor {
        MainProcessor {
            receiver,
            mappings: vec![],
            session,
        }
    }

    fn control(&self, mapping_id: MappingId, value: ControlValue) {
        let mapping = match self.mappings.get(mapping_id.index() as usize) {
            None => return,
            Some(m) => m,
        };
        mapping.control(value);
    }
}

#[derive(Debug)]
pub enum MainProcessorTask {
    UpdateMappings(Vec<MainProcessorMapping>),
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}
