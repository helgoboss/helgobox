use crate::domain::{MainProcessorFeedbackMapping, MappingId};
use helgoboss_learn::MidiSourceValue;
use helgoboss_midi::RawShortMessage;
use std::collections::HashSet;
use std::time::{Duration, Instant};

const BUFFER_DURATION: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct FeedbackBuffer {
    last_buffer_start: Instant,
    mappings: Vec<MainProcessorFeedbackMapping>,
    mapping_ids: HashSet<MappingId>,
}

impl Default for FeedbackBuffer {
    fn default() -> Self {
        Self {
            last_buffer_start: Instant::now(),
            mappings: vec![],
            mapping_ids: Default::default(),
        }
    }
}

impl FeedbackBuffer {
    pub fn update_mappings(&mut self, mappings: Vec<MainProcessorFeedbackMapping>) {
        self.reset();
        self.mappings = mappings;
    }

    pub fn buffer_mapping_id(&mut self, mapping_id: MappingId) {
        self.mapping_ids.insert(mapping_id);
    }

    pub fn poll(&mut self) -> Option<Vec<MidiSourceValue<RawShortMessage>>> {
        if Instant::now() - self.last_buffer_start <= BUFFER_DURATION {
            return None;
        }
        let source_values: Vec<_> = self
            .mapping_ids
            .iter()
            .filter_map(|mapping_id| {
                let mapping = self.mappings.get(mapping_id.index() as usize)?;
                mapping.feedback()
            })
            .collect();
        self.reset();
        Some(source_values)
    }

    fn reset(&mut self) {
        self.mapping_ids.clear();
        self.last_buffer_start = Instant::now();
    }
}
