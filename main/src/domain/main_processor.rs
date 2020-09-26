use crate::domain::{
    CompoundMappingSource, CompoundMappingSourceValue, CompoundMappingTarget, DomainEvent,
    DomainEventHandler, FeedbackBuffer, FeedbackRealTimeTask, MainMapping, MappingActivationUpdate,
    MappingCompartment, MappingId, NormalRealTimeTask, ProcessorContext, ReaperTarget,
};
use crossbeam_channel::Sender;
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;
use helgoboss_learn::{ControlValue, MidiSource, MidiSourceValue, UnitValue};
use helgoboss_midi::RawShortMessage;
use reaper_high::Reaper;
use reaper_medium::ControlSurface;
use rx_util::UnitEvent;
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
    logger: slog::Logger,
    /// Contains all mappings except those where the target could not be resolved.
    mappings: EnumMap<MappingCompartment, HashMap<MappingId, MainMapping>>,
    feedback_buffer: FeedbackBuffer,
    feedback_subscriptions: EnumMap<MappingCompartment, FeedbackSubscriptions>,
    feedback_is_globally_enabled: bool,
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
    event_handler: EH,
    context: ProcessorContext,
    party_is_over_subject: LocalSubject<'static, (), ()>,
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
                UpdateAllMappings(compartment, mut mappings) => {
                    debug!(
                        self.logger,
                        "Updating {} {}...",
                        mappings.len(),
                        compartment
                    );
                    let mut unused_sources = self.currently_feedback_enabled_sources(compartment);
                    // Refresh and put into hash map in order to quickly look up mappings by ID
                    self.mappings[compartment] = mappings
                        .into_iter()
                        .map(|mut m| {
                            m.refresh_all(&self.context, &self.parameters);
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                            (m.id(), m)
                        })
                        .collect();
                    // Sync to real-time processor
                    let real_time_mappings = self.mappings[compartment]
                        .values()
                        .map(|m| m.splinter_real_time_mapping())
                        .collect();
                    self.normal_real_time_task_sender
                        .send(NormalRealTimeTask::UpdateAllMappings(
                            compartment,
                            real_time_mappings,
                        ))
                        .unwrap();
                    self.handle_feedback_after_batch_mapping_update(compartment, &unused_sources);
                    self.update_on_mappings();
                }
                RefreshAllTargets => {
                    debug!(self.logger, "Refreshing all targets...");
                    for compartment in MappingCompartment::into_enum_iter() {
                        let mut unused_sources =
                            self.currently_feedback_enabled_sources(compartment);
                        let mut mappings_with_active_targets =
                            HashSet::with_capacity(self.mappings[compartment].len());
                        for m in self.mappings[compartment].values_mut() {
                            let is_active = m.refresh_target(&self.context);
                            if is_active {
                                mappings_with_active_targets.insert(m.id());
                            }
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                        }
                        self.normal_real_time_task_sender
                            .send(NormalRealTimeTask::UpdateTargetActivations(
                                compartment,
                                mappings_with_active_targets,
                            ))
                            .unwrap();
                        self.handle_feedback_after_batch_mapping_update(
                            compartment,
                            &unused_sources,
                        );
                    }
                    self.update_on_mappings();
                }
                UpdateSingleMapping(compartment, mut mapping) => {
                    debug!(
                        self.logger,
                        "Updating single {} {:?}...",
                        compartment,
                        mapping.id()
                    );
                    // Refresh
                    mapping.refresh_all(&self.context, &self.parameters);
                    // Sync to real-time processor
                    self.normal_real_time_task_sender
                        .send(NormalRealTimeTask::UpdateSingleMapping(
                            compartment,
                            mapping.splinter_real_time_mapping(),
                        ))
                        .unwrap();
                    // (Re)subscribe to or unsubscribe from feedback
                    if self.feedback_is_globally_enabled {
                        match mapping.target() {
                            Some(CompoundMappingTarget::Reaper(target))
                                if mapping.feedback_is_effectively_on() =>
                            {
                                // (Re)subscribe
                                let subscription = send_feedback_when_target_value_changed(
                                    self.self_feedback_sender.clone(),
                                    compartment,
                                    mapping.id(),
                                    target,
                                );
                                self.feedback_subscriptions[compartment]
                                    .insert(mapping.id(), subscription);
                                self.send_feedback(mapping.feedback_if_enabled());
                            }
                            _ => {
                                // Unsubscribe (if the feedback was enabled before)
                                self.feedback_subscriptions[compartment].remove(&mapping.id());
                                // Indicate via feedback that this source is not in use anymore. But
                                // only if feedback was enabled before (otherwise this could
                                // overwrite the feedback value of
                                // another enabled mapping which has the same
                                // source).
                                let was_previously_enabled = self.mappings[compartment]
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
                    self.mappings[compartment].insert(mapping.id(), *mapping);
                    // TODO-low Mmh, iterating over all mappings might be a bit overkill here.
                    self.update_on_mappings();
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
                    debug!(self.logger, "Updating all parameters...");
                    self.parameters = parameters;
                    // Activation is only supported for primary mappings
                    let compartment = MappingCompartment::PrimaryMappings;
                    let mut activation_updates: Vec<MappingActivationUpdate> = vec![];
                    let mut unused_sources = self.currently_feedback_enabled_sources(compartment);
                    for m in &mut self.mappings[compartment].values_mut() {
                        if m.is_affected_by_parameters() {
                            m.refresh_activation(&self.parameters);
                            activation_updates.push(MappingActivationUpdate {
                                id: m.id(),
                                is_active: m.is_active(),
                            });
                        }
                        if m.feedback_is_effectively_on() {
                            // Mark source as used
                            unused_sources.remove(m.source());
                        }
                    }
                    self.process_activation_updates(
                        compartment,
                        activation_updates,
                        &unused_sources,
                    );
                }
                UpdateParameter { index, value } => {
                    debug!(self.logger, "Updating parameter {} to {}...", index, value);
                    let previous_value = self.parameters[index as usize];
                    self.parameters[index as usize] = value;
                    // Activation is only supported for primary mappings
                    let compartment = MappingCompartment::PrimaryMappings;
                    let mut unused_sources = self.currently_feedback_enabled_sources(compartment);
                    // In order to avoid a mutable borrow of mappings and an immutable borrow of
                    // parameters at the same time, we need to separate the mapping updates into
                    // READ (read new activation state) and WRITE (write new activation state).
                    // 1. Read
                    let activation_updates: Vec<MappingActivationUpdate> = self.mappings
                        [compartment]
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
                        // TODO-high This
                        if let Some(m) = self.mappings[compartment].get_mut(&upd.id) {
                            m.update_activation(upd.is_active);
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                        }
                    }
                    self.process_activation_updates(
                        compartment,
                        activation_updates,
                        &unused_sources,
                    )
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    self.feedback_is_globally_enabled = is_enabled;
                    if is_enabled {
                        for compartment in MappingCompartment::into_enum_iter() {
                            self.handle_feedback_after_batch_mapping_update(
                                compartment,
                                &HashSet::new(),
                            );
                        }
                    } else {
                        for compartment in MappingCompartment::into_enum_iter() {
                            self.feedback_subscriptions[compartment].clear();
                        }
                        self.feedback_buffer.reset_all();
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
                Control {
                    compartment,
                    mapping_id,
                    value,
                    options,
                } => {
                    if let Some(m) = self.mappings[compartment].get_mut(&mapping_id) {
                        // Most of the time, the main processor won't even receive a control
                        // instruction (from the real-time processor) for a mapping for which
                        // control is disabled, because the real-time processor doesn't process
                        // disabled mappings. But if control is (temporarily) disabled because a
                        // target condition is (temporarily) not met (e.g. "track must be
                        // selected") and the real-time processor doesn't yet know about it, there
                        // might be a short amount of time where we still receive control
                        // statements. We filter them here.
                        let feedback = m.control_if_enabled(value, options);
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
                Feedback(compartment, mapping_id) => {
                    self.feedback_buffer
                        .buffer_feedback_for_mapping(compartment, mapping_id);
                }
            }
        }
        // Send feedback as soon as buffered long enough
        if self.feedback_is_globally_enabled {
            if let Some(mapping_ids) = self.feedback_buffer.poll() {
                let source_values = mapping_ids.iter().filter_map(|(compartment, mapping_id)| {
                    let mapping = self.mappings[*compartment].get(mapping_id)?;
                    mapping.feedback_if_enabled()
                });
                self.send_feedback(source_values);
            }
        }
    }
}

impl<EH: DomainEventHandler> MainProcessor<EH> {
    pub fn new(
        parent_logger: &slog::Logger,
        self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        parameters: [f32; PLUGIN_PARAMETER_COUNT as usize],
        event_handler: EH,
        context: ProcessorContext,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) = crossbeam_channel::unbounded();
        MainProcessor {
            logger: parent_logger.new(slog::o!("struct" => "MainProcessor")),
            self_normal_sender,
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
            context,
            party_is_over_subject: Default::default(),
        }
    }

    pub fn activate(&self) {
        // Handle dynamic target changes and target activation depending on REAPER state.
        //
        // Whenever anything changes that just affects the main processor targets, resync all
        // targets to the main processor. We don't want to resync to the real-time processor
        // just because another track has been selected. First, it would reset any source state
        // (e.g. short/long press timers). Second, it wouldn't change anything about the sources.
        // We also don't want to resync modes to the main processor. First, it would reset any
        // mode state (e.g. throttling data). Second, it would - again - not result in any change.
        // There are several global conditions which affect whether feedback will be sent
        // from a target or not. Similar global conditions decide what exactly produces the
        // feedback values (e.g. when there's a target which uses <Selected track>,
        // then a track selection change changes the feedback value producer ... so
        // the main processor needs to unsubscribe from the old producer and
        // subscribe to the new one).
        let self_sender = self.self_normal_sender.clone();
        ReaperTarget::potential_static_change_events()
            .merge(ReaperTarget::potential_dynamic_change_events())
            // We have this explicit stop criteria because we listen to global REAPER events.
            .take_until(self.party_is_over_subject.clone())
            .subscribe(move |_| {
                self_sender.send(NormalMainTask::RefreshAllTargets).unwrap();
            });
    }

    fn process_activation_updates(
        &mut self,
        compartment: MappingCompartment,
        activation_updates: Vec<MappingActivationUpdate>,
        unused_sources: &HashSet<CompoundMappingSource>,
    ) {
        if activation_updates.is_empty() {
            return;
        }
        // Send feedback
        // TODO-low Feedback could be reduced to just the activation update mappings
        self.handle_feedback_after_batch_mapping_update(compartment, &unused_sources);
        // Communicate changes to real-time processor
        self.normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateMappingActivations(
                compartment,
                activation_updates,
            ))
            .unwrap();
        // Update on mappings
        // TODO-low Mmh, iterating over all mappings might be a bit overkill here.
        self.update_on_mappings();
    }

    fn update_on_mappings(&self) {
        let on_mappings = self
            .all_mappings()
            .filter(|m| m.is_effectively_on())
            .map(MainMapping::id)
            .collect();
        self.event_handler
            .handle_event(DomainEvent::UpdateOnMappings(on_mappings));
    }

    fn send_feedback(&self, source_values: impl IntoIterator<Item = CompoundMappingSourceValue>) {
        for v in source_values.into_iter() {
            self.feedback_real_time_task_sender
                .send(FeedbackRealTimeTask::Feedback(v))
                .unwrap();
        }
    }

    fn all_mappings(&self) -> impl Iterator<Item = &MainMapping> {
        MappingCompartment::into_enum_iter()
            .map(move |compartment| self.mappings[compartment].values())
            .flatten()
    }

    fn feedback_all(&self) -> Vec<CompoundMappingSourceValue> {
        self.all_mappings()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn feedback_all_in_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> Vec<CompoundMappingSourceValue> {
        self.mappings[compartment]
            .values()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn feedback_all_zero(&self) -> Vec<CompoundMappingSourceValue> {
        self.all_mappings()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| m.source().feedback(UnitValue::MIN))
            .collect()
    }

    fn currently_feedback_enabled_sources(
        &self,
        compartment: MappingCompartment,
    ) -> HashSet<CompoundMappingSource> {
        self.mappings[compartment]
            .values()
            .filter(|m| m.feedback_is_effectively_on())
            .map(|m| m.source().clone())
            .collect()
    }

    fn handle_feedback_after_batch_mapping_update(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: &HashSet<CompoundMappingSource>,
    ) {
        if !self.feedback_is_globally_enabled {
            return;
        }
        // Subscribe to target value changes for feedback. Before that, cancel all existing
        // subscriptions.
        self.feedback_subscriptions[compartment].clear();
        for m in self.mappings[compartment]
            .values()
            .filter(|m| m.feedback_is_effectively_on())
        {
            if let Some(CompoundMappingTarget::Reaper(target)) = m.target() {
                // Subscribe
                let subscription = send_feedback_when_target_value_changed(
                    self.self_feedback_sender.clone(),
                    compartment,
                    m.id(),
                    target,
                );
                self.feedback_subscriptions[compartment].insert(m.id(), subscription);
            }
        }
        // Send feedback instantly to reflect this change in mappings.
        // At first indicate via feedback the sources which are not in use anymore.
        for s in now_unused_sources {
            self.send_feedback(s.feedback(UnitValue::MIN));
        }
        // Then discard the current feedback buffer and send feedback for all new mappings which
        // are enabled.
        self.feedback_buffer.reset_all_in_compartment(compartment);
        self.send_feedback(self.feedback_all_in_compartment(compartment));
    }

    fn log_debug_info(&self, task_count: usize) {
        // Summary
        let msg = format!(
            "\n\
                        # Main processor\n\
                        \n\
                        - Total primary mapping count: {} \n\
                        - Enabled primary mapping count: {} \n\
                        - Primary mapping feedback subscription count: {} \n\
                        - Total controller mapping count: {} \n\
                        - Enabled controller mapping count: {} \n\
                        - Controller mapping feedback subscription count: {} \n\
                        - Feedback buffer length: {} \n\
                        - Normal task count: {} \n\
                        - Control task count: {} \n\
                        - Feedback task count: {} \n\
                        - Parameter values: {:?} \n\
                        ",
            self.mappings[MappingCompartment::PrimaryMappings].len(),
            self.mappings[MappingCompartment::PrimaryMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.feedback_subscriptions[MappingCompartment::PrimaryMappings].len(),
            self.mappings[MappingCompartment::ControllerMappings].len(),
            self.mappings[MappingCompartment::ControllerMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.feedback_subscriptions[MappingCompartment::ControllerMappings].len(),
            self.feedback_buffer.len(),
            task_count,
            self.control_task_receiver.len(),
            self.feedback_task_receiver.len(),
            self.parameters,
        );
        Reaper::get().show_console_msg(msg);
        // Detailled
        println!(
            "\n\
            # Main processor\n\
            \n\
            {:#?}
            ",
            self
        );
    }
}

fn send_feedback_when_target_value_changed(
    self_sender: Sender<FeedbackMainTask>,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    target: &ReaperTarget,
) -> FeedbackSubscriptionGuard {
    target
        .value_changed()
        .subscribe(move |_| {
            self_sender
                .send(FeedbackMainTask::Feedback(compartment, mapping_id))
                .unwrap();
        })
        .unsubscribe_when_dropped()
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(MappingCompartment, Vec<MainMapping>),
    /// Replaces the given mapping.
    // Boxed because much larger struct size than other variants.
    UpdateSingleMapping(MappingCompartment, Box<MainMapping>),
    RefreshAllTargets,
    UpdateAllParameters([f32; PLUGIN_PARAMETER_COUNT as usize]),
    UpdateParameter {
        index: u32,
        value: f32,
    },
    UpdateFeedbackIsGloballyEnabled(bool),
    FeedbackAll,
    LogDebugInfo,
    LearnSource(CompoundMappingSource),
}

/// A feedback-related task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackMainTask {
    Feedback(MappingCompartment, MappingId),
}

/// A control-related task (which is potentially sent very frequently).
pub enum ControlMainTask {
    Control {
        compartment: MappingCompartment,
        mapping_id: MappingId,
        value: ControlValue,
        options: ControlOptions,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ControlOptions {
    pub enforce_send_feedback_after_control: bool,
}

#[derive(Debug)]
pub struct MainProcessorTargetUpdate {
    pub id: MappingId,
    pub target: Option<CompoundMappingTarget>,
    pub target_is_active: bool,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping main processor...");
        self.party_is_over_subject.next(());
    }
}
