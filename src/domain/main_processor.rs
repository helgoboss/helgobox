use crate::domain::{
    MainProcessorControlMapping, MainProcessorFeedbackMapping, MappingId, Mode,
    RealTimeProcessorTask, ReaperTarget, SharedSession,
};
use helgoboss_learn::{ControlValue, MidiSource, Target};
use reaper_medium::ControlSurface;
use rxrust::prelude::*;

const BULK_SIZE: usize = 30;

#[derive(Debug)]
pub struct MainProcessor {
    control_mappings: Vec<MainProcessorControlMapping>,
    feedback_mappings: Vec<MainProcessorFeedbackMapping>,
    feedback_subscriptions: Vec<SubscriptionGuard<Box<dyn SubscriptionLike>>>,
    self_sender: crossbeam_channel::Sender<MainProcessorTask>,
    receiver: crossbeam_channel::Receiver<MainProcessorTask>,
    real_time_processor_sender: crossbeam_channel::Sender<RealTimeProcessorTask>,
    session: SharedSession,
}

impl ControlSurface for MainProcessor {
    fn run(&mut self) {
        for task in self.receiver.try_iter().take(BULK_SIZE) {
            use MainProcessorTask::*;
            match task {
                UpdateMappings {
                    control_mappings,
                    feedback_mappings,
                } => {
                    self.control_mappings = control_mappings;
                    self.feedback_subscriptions = self.subscribe_to_feedback(&feedback_mappings);
                    self.feedback_mappings = feedback_mappings;
                    // TODO-high CONTINUE Feedback:
                    // - Clear vector with previous RAII subscriptions
                    // - For each feedback mapping, subscribe to ReaperTarget::value_changed().
                    // - Add subscription to the RAII subscription vector
                    // - The closure captures the main processor sender and the mapping ID, nothing
                    //   more!
                    // - It sends a Feedback(mapping_id) task to the main processor (itself). It
                    //   doesn't process immediately because a queried target value might not be the
                    //   latest.
                    // - The main processor, when receiving that Feedback task, starts a 10ms
                    //   timeout (if not already started) and throws the mapping ID in the buffer
                    //   set.
                    // - As soon as the timeout is reached (idle calls), the main processor resets
                    //   the timer, clears the set, queries the target value for each buffered
                    //   mapping ID and calculates the MidiSourceValue
                    // - This whole vector will be sent to the real-time processor. This one picks
                    //   it up and simply sends it to the feedback device.
                }
                Control { mapping_id, value } => {
                    self.control(mapping_id, value);
                }
                Feedback(mapping_id) => {
                    self.feedback(mapping_id);
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
            feedback_mappings: vec![],
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

    fn feedback(&self, mapping_id: MappingId) {
        let mapping = match self.feedback_mappings.get(mapping_id.index() as usize) {
            None => return,
            Some(m) => m,
        };
        if let Some(source_value) = mapping.feedback() {
            self.real_time_processor_sender
                .send(RealTimeProcessorTask::Feedback(source_value));
        }
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
