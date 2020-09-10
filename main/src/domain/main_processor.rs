use crate::domain::{
    DomainEvent, DomainEventHandler, FeedbackBuffer, FeedbackRealTimeTask, MappingActivationUpdate,
    MappingId, NormalMainMapping, NormalMappingSource, NormalMappingSourceValue,
    NormalRealTimeTask, ReaperTarget,
};
use crossbeam_channel::Sender;
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, UnitValue};
use helgoboss_midi::RawShortMessage;
use reaper_high::Reaper;
use reaper_medium::ControlSurface;
use rxrust::prelude::*;
use slog::debug;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

const NORMAL_TASK_BULK_SIZE: usize = 32;
const FEEDBACK_TASK_BULK_SIZE: usize = 32;
const CONTROL_TASK_BULK_SIZE: usize = 32;

type FeedbackSubscriptionGuard = SubscriptionGuard<Box<dyn SubscriptionLike>>;
type FeedbackSubscriptions = HashMap<MappingId, FeedbackSubscriptionGuard>;

// TODO-low Making this a usize might save quite some code
pub const PLUGIN_PARAMETER_COUNT: u32 = 20;

#[derive(Debug)]
pub struct MainProcessor<EH: DomainEventHandler> {
    /// Contains all mappings except those where the target could not be resolved.
    mappings: HashMap<MappingId, NormalMainMapping>,
    feedback_buffer: FeedbackBuffer,
    feedback_subscriptions: FeedbackSubscriptions,
    feedback_is_globally_enabled: bool,
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
    event_handler: EH,
}

impl<EH: DomainEventHandler> ControlSurface for MainProcessor<EH> {
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
                            if m.feedback_is_effectively_on() {
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
                    for update in updates.into_iter() {
                        if let Some(m) = self.mappings.get_mut(&update.id) {
                            m.update_target(update);
                            if m.feedback_is_effectively_on() {
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
                    if self.feedback_is_globally_enabled {
                        match mapping.target() {
                            Some(target) if mapping.feedback_is_effectively_on() => {
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
                                // only if feedback was enabled before (otherwise this could
                                // overwrite the feedback value of
                                // another enabled mapping which has the same
                                // source).
                                let was_previously_enabled = self
                                    .mappings
                                    .get(&mapping.id())
                                    .map(|m| m.feedback_is_effectively_on())
                                    .contains(&true);
                                if was_previously_enabled {
                                    // We assume that there's no other enabled mapping with the same
                                    // source at this moment. It there is, it would be a weird setup
                                    // with two conflicting feedback value sources - this wouldn't
                                    // work well anyway.
                                    self.send_feedback(mapping.source().feedback(UnitValue::MIN));
                                }
                            }
                        };
                    }
                    // Update hash map entry
                    self.mappings.insert(mapping.id(), *mapping);
                }
                FeedbackAll => {
                    if self.feedback_is_globally_enabled {
                        self.send_feedback(self.feedback_all());
                    }
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                LearnSource(source) => {
                    self.event_handler
                        .handle_event(DomainEvent::LearnedSource(source));
                }
                UpdateAllParameters(parameters) => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating all parameters..."
                    );
                    self.parameters = parameters;
                }
                UpdateParameter { index, value } => {
                    debug!(
                        Reaper::get().logger(),
                        "Main processor: Updating parameter..."
                    );
                    let previous_value = self.parameters[index as usize];
                    self.parameters[index as usize] = value;
                    let mut unused_sources = self.currently_feedback_enabled_sources();
                    // In order to avoid a mutable borrow of mappings and an immutable borrow of
                    // parameters at the same time, we need to separate the mapping updates into
                    // READ (read new activation state) and WRITE (write new activation state).
                    // 1. Read
                    let activation_updates: Vec<MappingActivationUpdate> = self
                        .mappings
                        .values()
                        .filter_map(|m| {
                            let result = m.notify_param_changed(
                                &self.parameters,
                                index,
                                previous_value,
                                value,
                            );
                            result.map(|is_active| MappingActivationUpdate {
                                id: m.id(),
                                is_active,
                            })
                        })
                        .collect();
                    // 2. Write
                    for upd in activation_updates.iter() {
                        if let Some(m) = self.mappings.get_mut(&upd.id) {
                            m.update_activation(upd.is_active);
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                        }
                    }
                    self.handle_feedback_after_batch_mapping_update(&unused_sources);
                    if !activation_updates.is_empty() {
                        self.normal_real_time_task_sender
                            .send(NormalRealTimeTask::UpdateMappingActivations(
                                activation_updates,
                            ))
                            .unwrap();
                    }
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    self.feedback_is_globally_enabled = is_enabled;
                    if is_enabled {
                        self.handle_feedback_after_batch_mapping_update(&HashSet::new());
                    } else {
                        self.feedback_subscriptions.clear();
                        self.feedback_buffer.reset();
                        self.send_feedback(self.feedback_all_zero());
                    }
                }
            }
        }
        // Process control tasks
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
                        let feedback = m.control_if_enabled(value);
                        self.send_feedback(feedback);
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
        if self.feedback_is_globally_enabled {
            if let Some(mapping_ids) = self.feedback_buffer.poll() {
                let source_values = mapping_ids.iter().filter_map(|mapping_id| {
                    let mapping = self.mappings.get(mapping_id)?;
                    mapping.feedback_if_enabled()
                });
                self.send_feedback(source_values);
            }
        }
    }
}

impl<EH: DomainEventHandler> MainProcessor<EH> {
    pub fn new(
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
        event_handler: EH,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) = crossbeam_channel::unbounded();
        MainProcessor {
            self_feedback_sender,
            normal_task_receiver,
            feedback_task_receiver,
            control_task_receiver,
            normal_real_time_task_sender,
            feedback_real_time_task_sender,
            mappings: Default::default(),
            feedback_buffer: Default::default(),
            feedback_subscriptions: Default::default(),
            feedback_is_globally_enabled: false,
            parameters,
            event_handler,
        }
    }

    fn send_feedback(&self, source_values: impl IntoIterator<Item = NormalMappingSourceValue>) {
        for v in source_values.into_iter() {
            self.feedback_real_time_task_sender
                .send(FeedbackRealTimeTask::Feedback(v))
                .unwrap();
        }
    }

    fn feedback_all(&self) -> Vec<NormalMappingSourceValue> {
        self.mappings
            .values()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn feedback_all_zero(&self) -> Vec<NormalMappingSourceValue> {
        self.mappings
            .values()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| m.source().feedback(UnitValue::MIN))
            .collect()
    }

    fn currently_feedback_enabled_sources(&self) -> HashSet<NormalMappingSource> {
        self.mappings
            .values()
            .filter(|m| m.feedback_is_effectively_on())
            .map(|m| m.source().clone())
            .collect()
    }

    fn handle_feedback_after_batch_mapping_update(
        &mut self,
        now_unused_sources: &HashSet<NormalMappingSource>,
    ) {
        if !self.feedback_is_globally_enabled {
            return;
        }
        // Subscribe to target value changes for feedback. Before that, cancel all existing
        // subscriptions.
        self.feedback_subscriptions.clear();
        for m in self
            .mappings
            .values()
            .filter(|m| m.feedback_is_effectively_on())
        {
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
        let msg = format!(
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
                        - Parameter values: {:?} \n\
                        ",
            // self.mappings.values(),
            self.mappings.len(),
            self.mappings
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.feedback_subscriptions.len(),
            self.feedback_buffer.len(),
            task_count,
            self.control_task_receiver.len(),
            self.feedback_task_receiver.len(),
            self.parameters,
        );
        Reaper::get().show_console_msg(msg);
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
            self_sender
                .send(FeedbackMainTask::Feedback(mapping_id))
                .unwrap();
        })
        .unsubscribe_when_dropped()
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(Vec<NormalMainMapping>),
    /// Replaces the given mapping.
    // Boxed because much larger struct size than other variants.
    UpdateSingleMapping(Box<NormalMainMapping>),
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
    UpdateAllParameters([f32; PLUGIN_PARAMETER_COUNT as usize]),
    UpdateParameter {
        index: u32,
        value: f32,
    },
    UpdateFeedbackIsGloballyEnabled(bool),
    FeedbackAll,
    LogDebugInfo,
    LearnSource(NormalMappingSource),
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
    pub target_is_active: bool,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping main processor...");
    }
}
