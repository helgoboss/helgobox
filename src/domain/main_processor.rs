use crate::domain::{
    FeedbackBuffer, MainProcessorControlMapping, MainProcessorFeedbackMapping, MappingId, Mode,
    RealTimeProcessorTask, ReaperTarget, SharedSession,
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
    control_mappings: HashMap<MappingId, MainProcessorControlMapping>,
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
                UpdateAllMappings {
                    control_mappings,
                    feedback_mappings,
                } => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all mappings..."
                    );
                    self.control_mappings =
                        control_mappings.into_iter().map(|m| (m.id(), m)).collect();
                    resubscribe_to_feedback_all(
                        &mut self.feedback_subscriptions,
                        &feedback_mappings,
                        self.self_sender.clone(),
                    );
                    let source_values = self.feedback_buffer.update_mappings(feedback_mappings);
                    self.send_feedback(source_values);
                }
                UpdateMapping {
                    id,
                    control_mapping,
                    feedback_mapping,
                } => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating mapping {:?}...", id
                    );
                    match control_mapping {
                        None => self.control_mappings.remove(&id),
                        Some(m) => self.control_mappings.insert(id, m),
                    };
                    resubscribe_to_feedback(
                        &mut self.feedback_subscriptions,
                        id,
                        feedback_mapping.as_ref(),
                        self.self_sender.clone(),
                    );
                    let source_value = self.feedback_buffer.update_mapping(id, feedback_mapping);
                    self.send_feedback(source_value);
                }
                Control { mapping_id, value } => {
                    self.control(mapping_id, value);
                }
                Feedback(mapping_id) => {
                    self.feedback_buffer.buffer_feedback_for_mapping(mapping_id);
                }
                FeedbackAll => self.send_feedback(self.feedback_buffer.feedback_all()),
                LearnSource(source) => {
                    self.session.borrow_mut().learn_source(source);
                }
            }
        }
        // Send feedback as soon as buffered long enough
        if let Some(source_values) = self.feedback_buffer.poll() {
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
            control_mappings: Default::default(),
            feedback_buffer: Default::default(),
            feedback_subscriptions: Default::default(),
            session,
        }
    }

    fn control(&self, mapping_id: MappingId, value: ControlValue) {
        let mapping = match self.control_mappings.get(&mapping_id) {
            None => return,
            Some(m) => m,
        };
        mapping.control(value);
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
}

fn send_feedback_when_target_value_changed(
    self_sender: Sender<MainProcessorTask>,
    m: &MainProcessorFeedbackMapping,
) -> FeedbackSubscriptionGuard {
    let mapping_id = m.id();
    m.target_value_changed()
        .subscribe(move |_| {
            self_sender.send(MainProcessorTask::Feedback(mapping_id));
        })
        .unsubscribe_when_dropped()
}

fn resubscribe_to_feedback(
    subscriptions: &mut FeedbackSubscriptions,
    id: MappingId,
    mapping: Option<&MainProcessorFeedbackMapping>,
    self_sender: crossbeam_channel::Sender<MainProcessorTask>,
) {
    match mapping {
        None => {
            subscriptions.remove(&id);
        }
        Some(m) => {
            let subscription = send_feedback_when_target_value_changed(self_sender, m);
            subscriptions.insert(m.id(), subscription);
        }
    }
}

fn resubscribe_to_feedback_all(
    subscriptions: &mut FeedbackSubscriptions,
    feedback_mappings: &Vec<MainProcessorFeedbackMapping>,
    self_sender: crossbeam_channel::Sender<MainProcessorTask>,
) {
    subscriptions.clear();
    for m in feedback_mappings {
        resubscribe_to_feedback(subscriptions, m.id(), Some(m), self_sender.clone());
    }
}

#[derive(Debug)]
pub enum MainProcessorTask {
    UpdateAllMappings {
        control_mappings: Vec<MainProcessorControlMapping>,
        feedback_mappings: Vec<MainProcessorFeedbackMapping>,
    },
    UpdateMapping {
        id: MappingId,
        control_mapping: Option<MainProcessorControlMapping>,
        feedback_mapping: Option<MainProcessorFeedbackMapping>,
    },
    Feedback(MappingId),
    FeedbackAll,
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}
