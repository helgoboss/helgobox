use crate::domain::{
    FeedbackBuffer, MainProcessorControlMapping, MainProcessorFeedbackMapping, MappingId, Mode,
    RealTimeProcessorTask, ReaperTarget, SharedSession,
};
use helgoboss_learn::{ControlValue, MidiSource, Target};
use reaper_medium::ControlSurface;
use rxrust::prelude::*;

const BULK_SIZE: usize = 30;

#[derive(Debug)]
pub struct MainProcessor {
    control_mappings: Vec<MainProcessorControlMapping>,
    feedback_buffer: FeedbackBuffer,
    feedback_subscriptions: Vec<SubscriptionGuard<Box<dyn SubscriptionLike>>>,
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
                UpdateMappings {
                    control_mappings,
                    feedback_mappings,
                } => {
                    self.control_mappings = control_mappings;
                    self.feedback_subscriptions = self.subscribe_to_feedback(&feedback_mappings);
                    self.feedback_buffer.update_mappings(feedback_mappings);
                }
                Control { mapping_id, value } => {
                    self.control(mapping_id, value);
                }
                Feedback(mapping_id) => {
                    self.feedback_buffer.buffer_mapping_id(mapping_id);
                }
                LearnSource(source) => {
                    self.session.borrow_mut().learn_source(&source);
                }
            }
        }
        // Send feedback as soon as buffered long enough
        if let Some(source_values) = self.feedback_buffer.poll() {
            for v in source_values {
                self.real_time_processor_sender
                    .send(RealTimeProcessorTask::Feedback(v));
            }
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
            control_mappings: vec![],
            feedback_buffer: Default::default(),
            feedback_subscriptions: vec![],
            session,
        }
    }

    fn control(&self, mapping_id: MappingId, value: ControlValue) {
        let mapping = match self.control_mappings.get(mapping_id.index() as usize) {
            None => return,
            Some(m) => m,
        };
        mapping.control(value);
    }

    fn subscribe_to_feedback(
        &self,
        feedback_mappings: &Vec<MainProcessorFeedbackMapping>,
    ) -> Vec<SubscriptionGuard<Box<dyn SubscriptionLike>>> {
        feedback_mappings
            .iter()
            .map(|m| {
                let self_sender = self.self_sender.clone();
                let mapping_id = m.mapping_id();
                m.target_value_changed()
                    .subscribe(move |_| {
                        self_sender.send(MainProcessorTask::Feedback(mapping_id));
                    })
                    .unsubscribe_when_dropped()
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum MainProcessorTask {
    UpdateMappings {
        control_mappings: Vec<MainProcessorControlMapping>,
        feedback_mappings: Vec<MainProcessorFeedbackMapping>,
    },
    Feedback(MappingId),
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}
