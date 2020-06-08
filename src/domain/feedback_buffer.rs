use crate::domain::{MainProcessorFeedbackMapping, MappingId};
use helgoboss_learn::MidiSourceValue;
use helgoboss_midi::RawShortMessage;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const BUFFER_DURATION: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct FeedbackBuffer {
    last_buffer_start: Instant,
    mappings: HashMap<MappingId, MainProcessorFeedbackMapping>,
    buffered_mapping_ids: HashSet<MappingId>,
}

impl Default for FeedbackBuffer {
    fn default() -> Self {
        Self {
            last_buffer_start: Instant::now(),
            mappings: Default::default(),
            buffered_mapping_ids: Default::default(),
        }
    }
}

impl FeedbackBuffer {
    pub fn update_mappings(
        &mut self,
        mappings: Vec<MainProcessorFeedbackMapping>,
    ) -> Vec<MidiSourceValue<RawShortMessage>> {
        self.reset();
        self.mappings = mappings.into_iter().map(|m| (m.id(), m)).collect();
        self.feedback_all()
    }

    pub fn feedback_all(&self) -> Vec<MidiSourceValue<RawShortMessage>> {
        self.mappings
            .values()
            .filter_map(|m| m.feedback())
            .collect()
    }

    pub fn update_mapping(
        &mut self,
        id: MappingId,
        mapping: Option<MainProcessorFeedbackMapping>,
    ) -> Option<MidiSourceValue<RawShortMessage>> {
        match mapping {
            None => {
                self.mappings.remove(&id);
                None
            }
            Some(m) => {
                let feedback = m.feedback();
                self.mappings.insert(id, m);
                feedback
            }
        }
    }

    pub fn buffer_feedback_for_mapping(&mut self, mapping_id: MappingId) {
        self.buffered_mapping_ids.insert(mapping_id);
    }

    pub fn poll(&mut self) -> Option<Vec<MidiSourceValue<RawShortMessage>>> {
        if Instant::now() - self.last_buffer_start <= BUFFER_DURATION {
            return None;
        }
        let source_values: Vec<_> = self
            .buffered_mapping_ids
            .iter()
            .filter_map(|mapping_id| {
                let mapping = self.mappings.get(mapping_id)?;
                mapping.feedback()
            })
            .collect();
        self.reset();
        Some(source_values)
    }

    fn reset(&mut self) {
        self.buffered_mapping_ids.clear();
        self.last_buffer_start = Instant::now();
    }
}
