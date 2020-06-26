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
use slog::{debug, info};
use smallvec::SmallVec;
use std::collections::HashMap;

const BULK_SIZE: usize = 32;

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
        // We could also iterate directly while keeping the receiver open. But that would (for good
        // reason) prevent us from calling other methods that mutably borrow self. To at least avoid
        // heap allocations, we use a smallvec.
        let tasks: SmallVec<[MainProcessorTask; BULK_SIZE]> =
            self.receiver.try_iter().take(BULK_SIZE).collect();
        for task in tasks {
            use MainProcessorTask::*;
            match task {
                UpdateAllMappings(mappings) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all mappings..."
                    );
                    // Put into hash map in order to quickly look up mappings by ID
                    self.mappings = mappings.into_iter().map(|m| (m.id(), m)).collect();
                    self.process_batch_mapping_update();
                }
                UpdateAllTargets(targets) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all targets..."
                    );
                    for t in targets.into_iter() {
                        if let Some(m) = self.mappings.get_mut(&t.id) {
                            m.update_from_target(t);
                        }
                    }
                    self.process_batch_mapping_update();
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
                    if let Some(m) = self.mappings.get_mut(&mapping_id) {
                        // Most of the time, the main processor won't even receive a control
                        // instruction (from the real-time processor) for a
                        // mapping for which control is disabled,
                        // because the real-time processor only ever gets mappings for which control
                        // is enabled. But if control is (temporarily) disabled because a target
                        // condition is (temporarily) not met (e.g. "track must be selected"), the
                        // real-time processor won't know about it (there's no resync to the
                        // real-time processor in this case in order too not
                        // reset source state like long/short press just
                        // because of a selection change). If we want the
                        // real-time processor to know about it (e.g. in order to reduce
                        // the amount of sources it has to process), we would need to build a more
                        // advanced syncing mechanism that uses diffs and retains sources.
                        // TODO-low Optimize if it causes performance issues, which I don't think.
                        m.control_if_enabled(value);
                    };
                }
                Feedback(mapping_id) => {
                    self.feedback_buffer.buffer_feedback_for_mapping(mapping_id);
                }
                FeedbackAll => {
                    self.send_feedback(self.feedback_all());
                }
                LogDebugInfo => {
                    self.log_debug_info();
                }
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

    fn process_batch_mapping_update(&mut self) {
        // Resubscribe to target value changes for feedback
        self.feedback_subscriptions.clear();
        for m in self.mappings.values().filter(|m| m.feedback_is_enabled()) {
            let subscription = send_feedback_when_target_value_changed(self.self_sender.clone(), m);
            self.feedback_subscriptions.insert(m.id(), subscription);
        }
        // Also send feedback instantly to reflect this change in mappings.
        self.feedback_buffer.reset();
        self.send_feedback(self.feedback_all());
    }

    fn log_debug_info(&self) {
        info!(
            Reaper::get().logger(),
            "\n\
                        # Main processor\n\
                        \n\
                        - Feedback subscription count: {} \n\
                        - Feedback buffer length: {} \n\
                        - Main processor task queue length: {} \n\
                        - Mapping count: {} \n\
                        ",
            // self.mappings.values(),
            self.feedback_subscriptions.len(),
            self.feedback_buffer.len(),
            self.receiver.len(),
            self.mappings.len(),
        );
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
    /// Use this whenever existing modes should not be overwritten.
    ///
    /// This is always the case when syncing as a result of ReaLearn control processing (e.g.
    /// when a selected track changes because a controller knob has been moved). Syncing the modes
    /// in such cases would reset all mutable mode state (e.g. throttling counter). Clearly
    /// undesired.
    UpdateAllTargets(Vec<MainProcessorTargetUpdate>),
    Feedback(MappingId),
    FeedbackAll,
    LogDebugInfo,
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
    LearnSource(MidiSource),
}

#[derive(Debug)]
pub struct MainProcessorTargetUpdate {
    pub id: MappingId,
    pub target: ReaperTarget,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
}
