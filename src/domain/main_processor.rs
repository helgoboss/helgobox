use crate::domain::{
    FeedbackBuffer, MainProcessorMapping, MappingId, Mode, RealTimeProcessorTask, ReaperTarget,
    SharedSession,
};
use crossbeam_channel::Sender;
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, Target};
use helgoboss_midi::RawShortMessage;
use reaper_high::Reaper;
use reaper_medium::ControlSurface;
use rxrust::prelude::*;
use slog::debug;
use std::collections::HashMap;

const BULK_SIZE: usize = 30;

type FeedbackSubscriptionGuard = SubscriptionGuard<Box<dyn SubscriptionLike>>;
type FeedbackSubscriptions = HashMap<MappingId, FeedbackSubscriptionGuard>;

#[derive(Debug)]
pub struct MainProcessor {
    mappings: HashMap<MappingId, MainProcessorMapping>,
    feedback_buffer: FeedbackBuffer,
    feedback_subscriptions: FeedbackSubscriptions,
    self_sender: crossbeam_channel::Sender<MainProcessorTask>,
    receiver: crossbeam_channel::Receiver<MainProcessorTask>,
    real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
    session: SharedSession,
}

impl ControlSurface for MainProcessor {
    fn run(&mut self) {
        // Process tasks
        for task in self.receiver.try_iter().take(BULK_SIZE) {
            use MainProcessorTask::*;
            match task {
                UpdateAllMappings(mappings) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all mappings..."
                    );
                    // Resubscribe to target value changes for feedback
                    self.feedback_subscriptions.clear();
                    for m in mappings.iter().filter(|m| m.feedback_is_enabled()) {
                        let subscription =
                            send_feedback_when_target_value_changed(self.self_sender.clone(), m);
                        self.feedback_subscriptions.insert(m.id(), subscription);
                    }
                    // Also send feedback instantly to reflect this change in mappings.
                    self.feedback_buffer.reset();
                    self.send_feedback(self.feedback_all());
                    // Put into hash map in order to quickly look up mappings by ID
                    self.mappings = mappings.into_iter().map(|m| (m.id(), m)).collect();
                }
                UpdateSingleMapping { id, mapping } => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating mapping {:?}...", id
                    );
                    match mapping {
                        None => {
                            // This mapping is gone for good.
                            self.mappings.remove(&id);
                            // TODO-medium We could send a null-feedback here to switch off lights.
                        }
                        Some(m) => {
                            // Resubscribe to or unsubscribe from feedback
                            if m.feedback_is_enabled() {
                                // Resubscribe
                                let subscription = send_feedback_when_target_value_changed(
                                    self.self_sender.clone(),
                                    &m,
                                );
                                self.feedback_subscriptions.insert(m.id(), subscription);
                            } else {
                                // If the feedback was enabled before, this will unsubscribe.
                                self.feedback_subscriptions.remove(&m.id());
                            }
                            // Send feedback if enabled
                            self.send_feedback(m.feedback_if_enabled());
                            // Update hash map entry
                            self.mappings.insert(id, m);
                        }
                    }
                }
                Control { mapping_id, value } => {
                    let mut mapping = match self.mappings.get_mut(&mapping_id) {
                        None => return,
                        Some(m) => m,
                    };
                    // Most of the time, the main processor won't even receive a control instruction
                    // (from the real-time processor) for a mapping for which control is disabled,
                    // because the real-time processor only ever gets mappings for which control
                    // is enabled. Anyway, here we do a second check.
                    mapping.control_if_enabled(value);
                }
                Feedback(mapping_id) => {
                    self.feedback_buffer.buffer_feedback_for_mapping(mapping_id);
                }
                FeedbackAll => self.send_feedback(self.feedback_all()),
                LearnSource(source) => {
                    self.session.borrow_mut().learn_source(source);
                }
            }
        }
        // Send feedback as soon as buffered long enough
        if let Some(mapping_ids) = self.feedback_buffer.poll() {
            let source_values = mapping_ids.iter().filter_map(|mapping_id| {
                let mapping = self.mappings.get(mapping_id)?;
                mapping.feedback_if_enabled()
            });
            self.send_feedback(source_values);
        }
    }
}

impl MainProcessor {
    pub fn new(
        self_sender: crossbeam_channel::Sender<MainProcessorTask>,
        receiver: crossbeam_channel::Receiver<MainProcessorTask>,
        real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
        session: SharedSession,
    ) -> MainProcessor {
        MainProcessor {
            self_sender,
            receiver,
            real_time_processor_sender,
            mappings: Default::default(),
            feedback_buffer: Default::default(),
            feedback_subscriptions: Default::default(),
            session,
        }
    }

    fn send_feedback(
        &self,
        source_values: impl IntoIterator<Item = MidiSourceValue<RawShortMessage>>,
    ) {
        for v in source_values.into_iter() {
            self.real_time_processor_sender
                .send(RealTimeProcessorTask::Feedback(v));
        }
    }

    fn feedback_all(&self) -> Vec<MidiSourceValue<RawShortMessage>> {
        self.mappings
            .values()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }
}

fn send_feedback_when_target_value_changed(
    self_sender: Sender<MainProcessorTask>,
    m: &MainProcessorMapping,
) -> FeedbackSubscriptionGuard {
    let mapping_id = m.id();
    m.target_value_changed()
        .subscribe(move |_| {
            self_sender.send(MainProcessorTask::Feedback(mapping_id));
        })
        .unsubscribe_when_dropped()
}

#[derive(Debug)]
pub enum MainProcessorTask {
    UpdateAllMappings(Vec<MainProcessorMapping>),
    UpdateSingleMapping {
        id: MappingId,
        mapping: Option<MainProcessorMapping>,
    },
    Feedback(MappingId),
    FeedbackAll,
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}
