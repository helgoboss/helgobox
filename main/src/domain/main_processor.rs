use crate::domain::{
    FeedbackBuffer, FeedbackRealTimeTask, MainProcessorMapping, MappingId, Mode,
    NormalRealTimeTask, ReaperTarget, WeakSession,
};
use crossbeam_channel::Sender;
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, Target, UnitValue};
use helgoboss_midi::RawShortMessage;
use reaper_high::Reaper;
use reaper_medium::ControlSurface;
use rxrust::prelude::*;
use slog::{debug, info};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

const NORMAL_TASK_BULK_SIZE: usize = 32;
const FEEDBACK_TASK_BULK_SIZE: usize = 32;
const CONTROL_TASK_BULK_SIZE: usize = 32;

type FeedbackSubscriptionGuard = SubscriptionGuard<Box<dyn SubscriptionLike>>;
type FeedbackSubscriptions = HashMap<MappingId, FeedbackSubscriptionGuard>;

#[derive(Debug)]
pub struct MainProcessor {
    /// Contains all mappings except those where the target could not be resolved.
    mappings: HashMap<MappingId, MainProcessorMapping>,
    feedback_buffer: FeedbackBuffer,
    feedback_subscriptions: FeedbackSubscriptions,
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    session: WeakSession,
}

impl ControlSurface for MainProcessor {
    fn run(&mut self) {
        // Process normal tasks
        // We could also iterate directly while keeping the receiver open. But that would (for
        // good reason) prevent us from calling other methods that mutably borrow
        // self. To at least avoid heap allocations, we use a smallvec.
        let normal_tasks: SmallVec<[NormalMainTask; NORMAL_TASK_BULK_SIZE]> = self
            .normal_task_receiver
            .try_iter()
            .take(NORMAL_TASK_BULK_SIZE)
            .collect();
        let normal_task_count = normal_tasks.len();
        for task in normal_tasks {
            use NormalMainTask::*;
            match task {
                UpdateAllMappings(mappings) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all mappings..."
                    );
                    let mut unused_sources = self.currently_feedback_enabled_sources();
                    // Put into hash map in order to quickly look up mappings by ID
                    self.mappings = mappings
                        .into_iter()
                        .map(|m| {
                            if m.feedback_is_enabled() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                            (m.id(), m)
                        })
                        .collect();
                    self.handle_feedback_after_batch_mapping_update(&unused_sources);
                }
                UpdateAllTargets(updates) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all targets..."
                    );
                    let mut unused_sources = self.currently_feedback_enabled_sources();
                    for t in updates.into_iter() {
                        if let Some(m) = self.mappings.get_mut(&t.id) {
                            m.update_from_target(t);
                            if m.feedback_is_enabled() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                        } else {
                            panic!("Couldn't find mapping while updating all targets");
                        }
                    }
                    self.handle_feedback_after_batch_mapping_update(&unused_sources);
                }
                UpdateSingleMapping(mapping) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating mapping {:?}...",
                        mapping.id()
                    );
                    // (Re)subscribe to or unsubscribe from feedback
                    match mapping.target() {
                        Some(target) if mapping.feedback_is_enabled() => {
                            // (Re)subscribe
                            let subscription = send_feedback_when_target_value_changed(
                                self.self_feedback_sender.clone(),
                                mapping.id(),
                                target,
                            );
                            self.feedback_subscriptions
                                .insert(mapping.id(), subscription);
                            self.send_feedback(mapping.feedback_if_enabled());
                        }
                        _ => {
                            // Unsubscribe (if the feedback was enabled before)
                            self.feedback_subscriptions.remove(&mapping.id());
                            // Indicate via feedback that this source is not in use anymore. But
                            // only if feedback was enabled before (otherwise this could overwrite
                            // the feedback value of another enabled mapping which has the same
                            // source).
                            let was_previously_enabled = self
                                .mappings
                                .get(&mapping.id())
                                .map(|m| m.feedback_is_enabled())
                                .contains(&true);
                            if was_previously_enabled {
                                // We assume that there's no other enabled mapping with the same
                                // source at this moment. It there is, it would be a weird setup
                                // with two conflicting feedback value sources - this wouldn't work
                                // well anyway.
                                self.send_feedback(mapping.source().feedback(UnitValue::MIN));
                            }
                        }
                    };
                    // Update hash map entry
                    self.mappings.insert(mapping.id(), mapping);
                }
                FeedbackAll => {
                    self.send_feedback(self.feedback_all());
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                LearnSource(source) => {
                    self.session
                        .upgrade()
                        .expect("session not existing anymore")
                        .borrow_mut()
                        .learn_source(source);
                }
            }
        }
        // Process feedback tasks
        let control_tasks: SmallVec<[ControlMainTask; CONTROL_TASK_BULK_SIZE]> = self
            .control_task_receiver
            .try_iter()
            .take(CONTROL_TASK_BULK_SIZE)
            .collect();
        for task in control_tasks {
            use ControlMainTask::*;
            match task {
                Control { mapping_id, value } => {
                    if let Some(m) = self.mappings.get_mut(&mapping_id) {
                        // Most of the time, the main processor won't even receive a control
                        // instruction (from the real-time processor) for a mapping for which
                        // control is disabled, because the real-time processor doesn't process
                        // disabled mappings. But if control is (temporarily) disabled because a
                        // target condition is (temporarily) not met (e.g. "track must be
                        // selected") and the real-time processor doesn't yet know about it, there
                        // might be a short amount of time where we still receive control
                        // statements. We filter them here.
                        m.control_if_enabled(value);
                    };
                }
            }
        }
        // Process feedback tasks
        let feedback_tasks: SmallVec<[FeedbackMainTask; FEEDBACK_TASK_BULK_SIZE]> = self
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
            .collect();
        for task in feedback_tasks {
            use FeedbackMainTask::*;
            match task {
                Feedback(mapping_id) => {
                    self.feedback_buffer.buffer_feedback_for_mapping(mapping_id);
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
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        session: WeakSession,
    ) -> MainProcessor {
        let (self_feedback_sender, feedback_task_receiver) = crossbeam_channel::unbounded();
        MainProcessor {
            self_feedback_sender,
            normal_task_receiver,
            feedback_task_receiver,
            control_task_receiver,
            feedback_real_time_task_sender,
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
            self.feedback_real_time_task_sender
                .send(FeedbackRealTimeTask::Feedback(v));
        }
    }

    fn feedback_all(&self) -> Vec<MidiSourceValue<RawShortMessage>> {
        self.mappings
            .values()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn currently_feedback_enabled_sources(&self) -> HashSet<MidiSource> {
        self.mappings
            .values()
            .filter(|m| m.feedback_is_enabled())
            .map(|m| m.source().clone())
            .collect()
    }

    fn handle_feedback_after_batch_mapping_update(
        &mut self,
        now_unused_sources: &HashSet<MidiSource>,
    ) {
        // Subscribe to target value changes for feedback. Before that, cancel all existing
        // subscriptions.
        self.feedback_subscriptions.clear();
        for m in self.mappings.values().filter(|m| m.feedback_is_enabled()) {
            if let Some(target) = m.target() {
                // Subscribe
                let subscription = send_feedback_when_target_value_changed(
                    self.self_feedback_sender.clone(),
                    m.id(),
                    target,
                );
                self.feedback_subscriptions.insert(m.id(), subscription);
            }
        }
        // Send feedback instantly to reflect this change in mappings.
        // At first indicate via feedback the sources which are not in use anymore.
        for s in now_unused_sources {
            self.send_feedback(s.feedback(UnitValue::MIN));
        }
        // Then discard the current feedback buffer and send feedback for all new mappings which
        // are enabled.
        self.feedback_buffer.reset();
        self.send_feedback(self.feedback_all());
    }

    fn log_debug_info(&self, task_count: usize) {
        info!(
            Reaper::get().logger(),
            "\n\
                        # Main processor\n\
                        \n\
                        - Total mapping count: {} \n\
                        - Enabled mapping count: {} \n\
                        - Feedback subscription count: {} \n\
                        - Feedback buffer length: {} \n\
                        - Normal task count: {} \n\
                        - Control task count: {} \n\
                        - Feedback task count: {} \n\
                        ",
            // self.mappings.values(),
            self.mappings.len(),
            self.mappings
                .values()
                .filter(|m| m.control_is_enabled() || m.feedback_is_enabled())
                .count(),
            self.feedback_subscriptions.len(),
            self.feedback_buffer.len(),
            task_count,
            self.control_task_receiver.len(),
            self.feedback_task_receiver.len(),
        );
    }
}

fn send_feedback_when_target_value_changed(
    self_sender: Sender<FeedbackMainTask>,
    mapping_id: MappingId,
    target: &ReaperTarget,
) -> FeedbackSubscriptionGuard {
    target
        .value_changed()
        .subscribe(move |_| {
            self_sender.send(FeedbackMainTask::Feedback(mapping_id));
        })
        .unsubscribe_when_dropped()
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(Vec<MainProcessorMapping>),
    /// Replaces the given mapping.
    UpdateSingleMapping(MainProcessorMapping),
    /// Replaces the targets of all given mappings.
    ///
    /// Use this instead of `UpdateAllMappings` whenever existing modes should not be overwritten.
    /// Attention: This never adds or removes any mappings.
    ///
    /// This is always the case when syncing as a result of ReaLearn control processing (e.g.
    /// when a selected track changes because a controller knob has been moved). Syncing the modes
    /// in such cases would reset all mutable mode state (e.g. throttling counter). Clearly
    /// undesired.
    UpdateAllTargets(Vec<MainProcessorTargetUpdate>),
    FeedbackAll,
    LogDebugInfo,
    LearnSource(MidiSource),
}

/// A feedback-related task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackMainTask {
    Feedback(MappingId),
}

/// A control-related task (which is potentially sent very frequently).
pub enum ControlMainTask {
    Control {
        mapping_id: MappingId,
        value: ControlValue,
    },
}

#[derive(Debug)]
pub struct MainProcessorTargetUpdate {
    pub id: MappingId,
    pub target: Option<ReaperTarget>,
    pub control_is_enabled: bool,
    pub feedback_is_enabled: bool,
}

impl Drop for MainProcessor {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping main processor...");
    }
}
