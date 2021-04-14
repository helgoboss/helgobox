use crate::domain::{
    ActivationChange, AdditionalFeedbackEvent, BackboneState, CompoundMappingSource,
    CompoundMappingTarget, ControlInput, ControlMode, DeviceFeedbackOutput, DomainEvent,
    DomainEventHandler, ExtendedProcessorContext, FeedbackAudioHookTask, FeedbackOutput,
    FeedbackRealTimeTask, FeedbackValue, InstanceOrchestrationEvent, IoUpdatedEvent, MainMapping,
    MappingActivationEffect, MappingCompartment, MappingId, MidiDestination, MidiSource,
    NormalRealTimeTask, OscDeviceId, OscFeedbackTask, PartialControlMatch,
    PlayPosFeedbackResolution, ProcessorContext, QualifiedSource, RealFeedbackValue, RealSource,
    RealTimeSender, RealearnMonitoringFxParameterValueChangedEvent, ReaperTarget,
    SourceFeedbackValue, SourceReleasedEvent, TargetValueChangedEvent, VirtualSourceValue,
};
use enum_map::EnumMap;
use helgoboss_learn::{ControlValue, ModeControlOptions, OscSource, UnitValue};

use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::ReaperNormalizedFxParamValue;
use rosc::{OscMessage, OscPacket};
use slog::debug;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

// This can be come pretty big when multiple track volumes are adjusted at once.
const FEEDBACK_TASK_QUEUE_SIZE: usize = 20_000;
const NORMAL_TASK_BULK_SIZE: usize = 32;
const FEEDBACK_TASK_BULK_SIZE: usize = 64;
const CONTROL_TASK_BULK_SIZE: usize = 32;
const PARAMETER_TASK_BULK_SIZE: usize = 32;

pub const PLUGIN_PARAMETER_COUNT: u32 = 200;
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;
pub type ParameterArray = [f32; PLUGIN_PARAMETER_COUNT as usize];
pub type ParameterSlice = [f32];
pub const ZEROED_PLUGIN_PARAMETERS: ParameterArray = [0.0f32; PLUGIN_PARAMETER_COUNT as usize];

#[derive(Debug)]
pub struct MainProcessor<EH: DomainEventHandler> {
    instance_id: String,
    logger: slog::Logger,
    /// Contains mappings without virtual targets.
    mappings: EnumMap<MappingCompartment, HashMap<MappingId, MainMapping>>,
    /// Contains mappings with virtual targets.
    mappings_with_virtual_targets: HashMap<MappingId, MainMapping>,
    /// Contains IDs of those mappings which should be refreshed as soon as a target is touched.
    /// At the moment only "Last touched" targets.
    target_touch_dependent_mappings: EnumMap<MappingCompartment, HashSet<MappingId>>,
    /// Contains IDs of those mappings whose feedback might change depending on the current beat.
    beat_dependent_feedback_mappings: EnumMap<MappingCompartment, HashSet<MappingId>>,
    /// Contains IDs of those mappings whose feedback might change depending on the current milli.
    milli_dependent_feedback_mappings: EnumMap<MappingCompartment, HashSet<MappingId>>,
    /// Contains IDs of those mappings who need to be polled as frequently as possible.
    poll_control_mappings: EnumMap<MappingCompartment, HashSet<MappingId>>,
    // TODO-medium Now that we communicate the feedback output separately, we could limit the scope
    //  of its meaning to "instance enabled etc."
    feedback_is_globally_enabled: bool,
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
    feedback_real_time_task_sender: RealTimeSender<FeedbackRealTimeTask>,
    feedback_audio_hook_task_sender: RealTimeSender<FeedbackAudioHookTask>,
    osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
    additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    instance_orchestration_event_sender: crossbeam_channel::Sender<InstanceOrchestrationEvent>,
    parameters: ParameterArray,
    event_handler: EH,
    context: ProcessorContext,
    control_mode: ControlMode,
    control_is_globally_enabled: bool,
    control_input: ControlInput,
    feedback_output: Option<FeedbackOutput>,
}

impl<EH: DomainEventHandler> MainProcessor<EH> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: String,
        parent_logger: &slog::Logger,
        self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
        feedback_real_time_task_sender: RealTimeSender<FeedbackRealTimeTask>,
        feedback_audio_hook_task_sender: RealTimeSender<FeedbackAudioHookTask>,
        additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
        instance_orchestration_event_sender: crossbeam_channel::Sender<InstanceOrchestrationEvent>,
        osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
        event_handler: EH,
        context: ProcessorContext,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) =
            crossbeam_channel::bounded(FEEDBACK_TASK_QUEUE_SIZE);
        let logger = parent_logger.new(slog::o!("struct" => "MainProcessor"));
        MainProcessor {
            instance_id,
            logger: logger.clone(),
            self_normal_sender,
            self_feedback_sender,
            normal_task_receiver,
            feedback_task_receiver,
            control_task_receiver,
            parameter_task_receiver,
            normal_real_time_task_sender,
            feedback_real_time_task_sender,
            mappings: Default::default(),
            mappings_with_virtual_targets: Default::default(),
            target_touch_dependent_mappings: Default::default(),
            beat_dependent_feedback_mappings: Default::default(),
            milli_dependent_feedback_mappings: Default::default(),
            poll_control_mappings: Default::default(),
            feedback_is_globally_enabled: false,
            parameters: ZEROED_PLUGIN_PARAMETERS,
            event_handler,
            context,
            control_mode: ControlMode::Controlling,
            control_is_globally_enabled: true,
            control_input: Default::default(),
            feedback_output: Default::default(),
            osc_feedback_task_sender,
            additional_feedback_event_sender,
            instance_orchestration_event_sender,
            feedback_audio_hook_task_sender,
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// This is the chance to take over a source from another instance (send our feedback).
    ///
    /// This is a very important principle when using multiple instances. It allows feedback to
    /// not be accidentally cleared while still guaranteeing that feedback for non-used control
    /// elements are cleared eventually - independently from the order of instance processing.
    pub fn maybe_takeover_source(&self, source: &RealSource) -> bool {
        if let Some(mapping_with_source) = self
            .all_mappings()
            .find(|m| m.feedback_is_effectively_on() && m.has_this_real_source(source))
        {
            if let Some(followed_mapping) = self.follow_maybe_virtual_mapping(mapping_with_source) {
                if self.feedback_is_effectively_enabled() {
                    debug!(self.logger, "Taking over source {:?}...", source);
                    let feedback = followed_mapping.feedback(true);
                    self.send_feedback(FeedbackReason::TakeOverSource, feedback);
                    true
                } else {
                    debug!(
                        self.logger,
                        "No source takeover of {:?} because feedback effectively disabled", source
                    );
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    /// We previously wanted to switch off lights for a source but gave other instances the chance
    /// to take over. This is called if no takeover happened and it's safe to really turn the lights
    /// off.
    pub fn finally_switch_off_source(
        &self,
        feedback_output: FeedbackOutput,
        feedback_value: SourceFeedbackValue,
    ) {
        debug!(
            self.logger,
            "Finally switching off source with {:?}...", feedback_value
        );
        send_direct_source_feedback(
            &self.instance_props(),
            feedback_output,
            FeedbackReason::FinallySwitchOffSource,
            feedback_value,
        );
    }

    /// This should be regularly called by the control surface in normal mode.
    pub fn run_all(&mut self) {
        self.run_essential();
        self.run_control();
    }

    /// Processes control tasks coming from the real-time processor.
    ///
    /// This should *not* be called by the control surface when it's globally learning targets
    /// because we want to pause controlling in that case! Otherwise we could control targets and
    /// they would be learned although not touched via mouse, that's not good.
    fn run_control(&mut self) {
        // Collect control tasks (we do that in any case to not let get channels full).
        let control_tasks: SmallVec<[ControlMainTask; CONTROL_TASK_BULK_SIZE]> = self
            .control_task_receiver
            .try_iter()
            .take(CONTROL_TASK_BULK_SIZE)
            .collect();
        // It's possible that control is disabled because another instance cancels us. In that case
        // the RealTimeProcessor won't know about it and keeps sending MIDI. Stop it here!
        if self.control_is_effectively_enabled() {
            for task in control_tasks {
                use ControlMainTask::*;
                match task {
                    Control {
                        compartment,
                        mapping_id,
                        value,
                        options,
                    } => {
                        // Resolving mappings with virtual targets is not necessary anymore. It has
                        // been done in the real-time processor already.
                        if let Some(m) = self.mappings[compartment].get_mut(&mapping_id) {
                            // Most of the time, the main processor won't even receive a
                            // MIDI-triggered control instruction from
                            // the real-time processor for a mapping for
                            // which control is disabled, because the
                            // real-time processor doesn't process
                            // disabled mappings. But if control is (temporarily) disabled because a
                            // target condition is (temporarily) not met (e.g. "track must be
                            // selected") and the real-time processor doesn't yet know about it,
                            // there might be a short amount of time
                            // where we still receive control
                            // statements. We filter them here.
                            let feedback = m.control_if_enabled(value, options);
                            self.send_feedback(FeedbackReason::Normal, feedback);
                        };
                    }
                }
            }
            for compartment in MappingCompartment::enum_iter() {
                for id in self.poll_control_mappings[compartment].iter() {
                    if let Some(m) = self.mappings[compartment].get_mut(id) {
                        let feedback = m.poll_if_control_enabled();
                        self.send_feedback(FeedbackReason::Normal, feedback);
                    }
                }
            }
        }
    }

    /// This should be regularly called by the control surface, even during global target learning.
    pub fn run_essential(&mut self) {
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
                UpdateSettings {
                    control_input,
                    feedback_output,
                } => {
                    let released_event = self.io_released_event();
                    self.control_input = control_input;
                    self.feedback_output = feedback_output;
                    let changed_event = self.feedback_output_usage_might_have_changed_event();
                    self.send_io_update(released_event).unwrap();
                    self.send_io_update(changed_event).unwrap();
                }
                UpdateAllMappings(compartment, mut mappings) => {
                    debug!(
                        self.logger,
                        "Updating {} {}...",
                        mappings.len(),
                        compartment
                    );
                    let mut unused_sources =
                        self.currently_feedback_enabled_sources(compartment, true);
                    self.target_touch_dependent_mappings[compartment].clear();
                    self.beat_dependent_feedback_mappings[compartment].clear();
                    self.milli_dependent_feedback_mappings[compartment].clear();
                    self.poll_control_mappings[compartment].clear();
                    // Refresh and splinter real-time mappings
                    let real_time_mappings = mappings
                        .iter_mut()
                        .map(|m| {
                            m.refresh_all(ExtendedProcessorContext::new(
                                &self.context,
                                &self.parameters,
                            ));
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(&m.qualified_source());
                            }
                            if m.needs_refresh_when_target_touched() {
                                self.target_touch_dependent_mappings[compartment].insert(m.id());
                            }
                            let influence = m.play_pos_feedback_resolution();
                            if influence == Some(PlayPosFeedbackResolution::Beat) {
                                self.beat_dependent_feedback_mappings[compartment].insert(m.id());
                            }
                            if influence == Some(PlayPosFeedbackResolution::High) {
                                self.milli_dependent_feedback_mappings[compartment].insert(m.id());
                            }
                            if m.wants_to_be_polled_for_control() {
                                self.poll_control_mappings[compartment].insert(m.id());
                            }
                            m.splinter_real_time_mapping()
                        })
                        .collect();
                    // Put into hash map in order to quickly look up mappings by ID
                    let mapping_tuples = mappings.into_iter().map(|m| (m.id(), m));
                    if compartment == MappingCompartment::ControllerMappings {
                        let (virtual_target_mappings, normal_mappings) =
                            mapping_tuples.partition(|(_, m)| m.has_virtual_target());
                        self.mappings[compartment] = normal_mappings;
                        self.mappings_with_virtual_targets = virtual_target_mappings;
                    } else {
                        self.mappings[compartment] = mapping_tuples.collect();
                    }
                    // Sync to real-time processor
                    self.normal_real_time_task_sender
                        .send(NormalRealTimeTask::UpdateAllMappings(
                            compartment,
                            real_time_mappings,
                        ))
                        .unwrap();
                    // Important to send IO event first ...
                    self.notify_feedback_dev_usage_might_have_changed(compartment);
                    // ... and then mapping update. Otherwise, if this is an upper-floor instance
                    // clearing all mappings, other instances won't see yet that they are actually
                    // allowed to take over sources! Which might delay the reactivation of
                    // lower-floor instances.
                    self.handle_feedback_after_having_updated_all_mappings(
                        compartment,
                        &unused_sources,
                    );
                    self.update_on_mappings();
                }
                // This is sent on events such as track list change, FX focus etc.
                RefreshAllTargets => {
                    debug!(self.logger, "Refreshing all targets...");
                    for compartment in MappingCompartment::enum_iter() {
                        let mut activation_updates: Vec<ActivationChange> = vec![];
                        let mut changed_mappings = vec![];
                        let mut unused_sources =
                            self.currently_feedback_enabled_sources(compartment, false);
                        // Mappings with virtual targets don't have to be refreshed because virtual
                        // targets are always active and never change depending on circumstances.
                        for m in self.mappings[compartment].values_mut() {
                            let context =
                                ExtendedProcessorContext::new(&self.context, &self.parameters);
                            let (target_changed, activation_update) = m.refresh_target(context);
                            if target_changed || activation_update.is_some() {
                                changed_mappings.push(m.id());
                            }
                            if let Some(u) = activation_update {
                                activation_updates.push(u);
                            }
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(&m.qualified_source());
                            }
                        }
                        if !activation_updates.is_empty() {
                            // In some cases like closing projects, it's possible that this will
                            // fail because the real-time processor is
                            // already gone. But it doesn't matter.
                            let _ = self.normal_real_time_task_sender.send(
                                NormalRealTimeTask::UpdateTargetActivations(
                                    compartment,
                                    activation_updates,
                                ),
                            );
                        }
                        // Important to send IO event first ...
                        self.notify_feedback_dev_usage_might_have_changed(compartment);
                        self.handle_feedback_after_having_updated_particular_mappings(
                            compartment,
                            &unused_sources,
                            changed_mappings.into_iter(),
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
                    mapping.refresh_all(ExtendedProcessorContext::new(
                        &self.context,
                        &self.parameters,
                    ));
                    // Sync to real-time processor
                    self.normal_real_time_task_sender
                        .send(NormalRealTimeTask::UpdateSingleMapping(
                            compartment,
                            Box::new(mapping.splinter_real_time_mapping()),
                        ))
                        .unwrap();
                    // Collect feedback (important to send later as soon as mappings updated)
                    struct Fb(FeedbackReason, Option<FeedbackValue>);
                    impl Fb {
                        fn none() -> Self {
                            Fb(FeedbackReason::Normal, None)
                        }

                        fn unused(value: Option<FeedbackValue>) -> Self {
                            Fb(FeedbackReason::ClearUnusedSource, value)
                        }

                        fn normal(value: Option<FeedbackValue>) -> Self {
                            Fb(FeedbackReason::Normal, value)
                        }
                    }

                    let (fb1, fb2) = if let Some(previous_mapping) =
                        self.get_normal_or_virtual_target_mapping(compartment, mapping.id())
                    {
                        // An existing mapping is being overwritten.
                        if previous_mapping.feedback_is_effectively_on() {
                            // And its light is currently on.
                            if mapping.source() == previous_mapping.source() {
                                // Source is the same.
                                if mapping.feedback_is_effectively_on() {
                                    // Lights should still be on.
                                    // Send new lights.
                                    (
                                        Fb::none(),
                                        Fb::normal(
                                            self.get_mapping_feedback_follow_virtual(&*mapping),
                                        ),
                                    )
                                } else {
                                    // Lights should now be off.
                                    (Fb::unused(mapping.zero_feedback()), Fb::none())
                                }
                            } else {
                                // Source has changed.
                                // Switch previous source light off.
                                let fb1 = Fb::unused(previous_mapping.zero_feedback());
                                let fb2 = if mapping.feedback_is_effectively_on() {
                                    // Lights should be on. Send new lights.
                                    Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping))
                                } else {
                                    Fb::none()
                                };
                                (fb1, fb2)
                            }
                        } else {
                            // Previous lights were off.
                            if mapping.feedback_is_effectively_on() {
                                // Now should be on.
                                (
                                    Fb::none(),
                                    Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping)),
                                )
                            } else {
                                // Still off.
                                (Fb::none(), Fb::none())
                            }
                        }
                    } else {
                        // This mapping is new.
                        if mapping.feedback_is_effectively_on() {
                            // Lights on.
                            (
                                Fb::none(),
                                Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping)),
                            )
                        } else {
                            // Lights off.
                            (Fb::none(), Fb::none())
                        }
                    };
                    // Update hash map entries
                    if mapping.needs_refresh_when_target_touched() {
                        self.target_touch_dependent_mappings[compartment].insert(mapping.id());
                    } else {
                        self.target_touch_dependent_mappings[compartment].remove(&mapping.id());
                    }
                    let influence = mapping.play_pos_feedback_resolution();
                    if influence == Some(PlayPosFeedbackResolution::Beat) {
                        self.beat_dependent_feedback_mappings[compartment].insert(mapping.id());
                    } else {
                        self.beat_dependent_feedback_mappings[compartment].remove(&mapping.id());
                    }
                    if influence == Some(PlayPosFeedbackResolution::High) {
                        self.milli_dependent_feedback_mappings[compartment].insert(mapping.id());
                    } else {
                        self.milli_dependent_feedback_mappings[compartment].remove(&mapping.id());
                    }
                    if mapping.wants_to_be_polled_for_control() {
                        self.poll_control_mappings[compartment].insert(mapping.id());
                    } else {
                        self.poll_control_mappings[compartment].remove(&mapping.id());
                    }
                    let relevant_map = if mapping.has_virtual_target() {
                        self.mappings[compartment].remove(&mapping.id());
                        &mut self.mappings_with_virtual_targets
                    } else {
                        self.mappings_with_virtual_targets.remove(&mapping.id());
                        &mut self.mappings[compartment]
                    };
                    relevant_map.insert(mapping.id(), *mapping);
                    // Send feedback
                    self.send_feedback(fb1.0, fb1.1);
                    self.send_feedback(fb1.0, fb2.1);
                    // TODO-low Mmh, iterating over all mappings might be a bit overkill here.
                    self.update_on_mappings();
                }
                SendAllFeedback => {
                    self.send_all_feedback();
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                LearnMidiSource {
                    source,
                    allow_virtual_sources,
                } => {
                    self.event_handler.handle_event(DomainEvent::LearnedSource {
                        source: RealSource::Midi(source),
                        allow_virtual_sources,
                    });
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    debug!(
                        self.logger,
                        "Updating feedback_is_globally_enabled to {}", is_enabled
                    );
                    self.feedback_is_globally_enabled = is_enabled;
                    if is_enabled {
                        for compartment in MappingCompartment::enum_iter() {
                            self.handle_feedback_after_having_updated_all_mappings(
                                compartment,
                                &HashSet::new(),
                            );
                        }
                    } else {
                        // Clear it completely. Other instances that might take over maybe don't use
                        // all control elements and we don't want to leave traces.
                        self.clear_all_feedback_allowing_source_takeover();
                    };
                    let event = self.feedback_output_usage_might_have_changed_event();
                    self.send_io_update(event).unwrap();
                }
                StartLearnSource {
                    allow_virtual_sources,
                    osc_arg_index_hint,
                } => {
                    debug!(self.logger, "Start learning source");
                    self.control_mode = ControlMode::LearningSource {
                        allow_virtual_sources,
                        osc_arg_index_hint,
                    };
                }
                DisableControl => {
                    debug!(self.logger, "Disable control");
                    self.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    debug!(self.logger, "Return to control mode");
                    self.control_mode = ControlMode::Controlling;
                }
                UpdateControlIsGloballyEnabled(is_enabled) => {
                    self.control_is_globally_enabled = is_enabled;
                    let event = IoUpdatedEvent {
                        ..self.basic_io_changed_event()
                    };
                    self.send_io_update(event).unwrap();
                }
                FullResyncToRealTimeProcessorPlease => {
                    // We cannot provide everything that the real-time processor needs so we need
                    // to delegate to the session in order to let it do the resync (could be
                    // changed by also holding unnecessary things but for now, why not taking the
                    // session detour).
                    self.event_handler
                        .handle_event(DomainEvent::FullResyncRequested);
                }
            }
        }
        // Process parameter tasks
        let parameter_tasks: SmallVec<[ParameterMainTask; PARAMETER_TASK_BULK_SIZE]> = self
            .parameter_task_receiver
            .try_iter()
            .take(PARAMETER_TASK_BULK_SIZE)
            .collect();
        for task in parameter_tasks {
            use ParameterMainTask::*;
            match task {
                UpdateAllParameters(parameters) => {
                    debug!(self.logger, "Updating all parameters...");
                    self.parameters = *parameters;
                    self.event_handler
                        .handle_event(DomainEvent::UpdatedAllParameters(parameters));
                    for compartment in MappingCompartment::enum_iter() {
                        let mut mapping_activation_changes: Vec<ActivationChange> = vec![];
                        let mut target_activation_changes: Vec<ActivationChange> = vec![];
                        let mut changed_mappings = vec![];
                        let mut unused_sources =
                            self.currently_feedback_enabled_sources(compartment, true);
                        for m in all_mappings_in_compartment_mut(
                            &mut self.mappings,
                            &mut self.mappings_with_virtual_targets,
                            compartment,
                        ) {
                            if m.activation_can_be_affected_by_parameters() {
                                if let Some(update) = m.update_activation(&self.parameters) {
                                    mapping_activation_changes.push(update);
                                }
                            }
                            if m.target_can_be_affected_by_parameters() {
                                let context =
                                    ExtendedProcessorContext::new(&self.context, &self.parameters);
                                let (has_changed, activation_change) = m.refresh_target(context);
                                if has_changed || activation_change.is_some() {
                                    changed_mappings.push(m.id())
                                }
                                if let Some(u) = activation_change {
                                    target_activation_changes.push(u);
                                }
                            }
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(&m.qualified_source());
                            }
                        }
                        self.process_mapping_updates_due_to_parameter_changes(
                            compartment,
                            mapping_activation_changes,
                            target_activation_changes,
                            &unused_sources,
                            changed_mappings.into_iter(),
                        );
                    }
                }
                UpdateParameter { index, value } => {
                    debug!(self.logger, "Updating parameter {} to {}...", index, value);
                    // Work around REAPER's inability to notify about parameter changes in
                    // monitoring FX by simulating the notification ourselves.
                    // Then parameter learning and feedback works at least for
                    // ReaLearn monitoring FX instances, which is especially
                    // useful for conditional activation.
                    if self.context.is_on_monitoring_fx_chain() {
                        let parameter = self.context.containing_fx().parameter_by_index(index);
                        self.additional_feedback_event_sender
                            .try_send(
                                AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(
                                    RealearnMonitoringFxParameterValueChangedEvent {
                                        parameter,
                                        new_value: ReaperNormalizedFxParamValue::new(value as _),
                                    },
                                ),
                            )
                            .unwrap();
                    }
                    // Update own value (important to do first)
                    let previous_value = self.parameters[index as usize];
                    self.parameters[index as usize] = value;
                    self.event_handler
                        .handle_event(DomainEvent::UpdatedParameter { index, value });
                    // Mapping activation is supported for both compartments and target activation
                    // might change also in non-virtual controller mappings due to dynamic targets.
                    if let Some(compartment) = MappingCompartment::by_absolute_param_index(index) {
                        let mut changed_mappings = HashSet::new();
                        let mut unused_sources =
                            self.currently_feedback_enabled_sources(compartment, true);
                        // In order to avoid a mutable borrow of mappings and an immutable borrow of
                        // parameters at the same time, we need to separate into READ activation
                        // effects and WRITE activation updates.
                        // 1. Mapping activation: Read
                        let activation_effects: Vec<MappingActivationEffect> = self
                            .all_mappings_in_compartment(compartment)
                            .filter_map(|m| {
                                m.check_activation_effect(&self.parameters, index, previous_value)
                            })
                            .collect();
                        // 2. Mapping activation: Write
                        let mapping_activation_updates: Vec<ActivationChange> = activation_effects
                            .into_iter()
                            .filter_map(|eff| {
                                changed_mappings.insert(eff.id);
                                let m = get_normal_or_virtual_target_mapping_mut(
                                    &mut self.mappings,
                                    &mut self.mappings_with_virtual_targets,
                                    compartment,
                                    eff.id,
                                )?;
                                m.update_activation_from_effect(eff)
                            })
                            .collect();
                        // 3. Target refreshment and determine unused sources
                        let mut target_activation_changes: Vec<ActivationChange> = vec![];
                        for m in all_mappings_in_compartment_mut(
                            &mut self.mappings,
                            &mut self.mappings_with_virtual_targets,
                            compartment,
                        ) {
                            if m.target_can_be_affected_by_parameters() {
                                let context =
                                    ExtendedProcessorContext::new(&self.context, &self.parameters);
                                let (target_has_changed, activation_change) =
                                    m.refresh_target(context);
                                if target_has_changed || activation_change.is_some() {
                                    changed_mappings.insert(m.id());
                                }
                                if let Some(c) = activation_change {
                                    target_activation_changes.push(c);
                                }
                            }
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(&m.qualified_source());
                            }
                        }
                        self.process_mapping_updates_due_to_parameter_changes(
                            compartment,
                            mapping_activation_updates,
                            target_activation_changes,
                            &unused_sources,
                            changed_mappings.into_iter(),
                        )
                    }
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
                TargetTouched => {
                    for compartment in MappingCompartment::enum_iter() {
                        for mapping_id in self.target_touch_dependent_mappings[compartment].iter() {
                            // Virtual targets are not candidates for "Last touched" so we don't
                            // need to consider them here.
                            let fb =
                                if let Some(m) = self.mappings[compartment].get_mut(&mapping_id) {
                                    // We don't need to track activation updates because this target
                                    // is always on. Switching off is not necessary since the last
                                    // touched target can never be "unset".
                                    m.refresh_target(ExtendedProcessorContext::new(
                                        &self.context,
                                        &self.parameters,
                                    ));
                                    if let Some(CompoundMappingTarget::Reaper(_)) = m.target() {
                                        if m.feedback_is_effectively_on() {
                                            m.feedback(true)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                            self.send_feedback(FeedbackReason::Normal, fb);
                        }
                    }
                }
            }
        }
        // Process high-resolution playback-position dependent feedback
        for compartment in MappingCompartment::enum_iter() {
            for mapping_id in self.milli_dependent_feedback_mappings[compartment].iter() {
                if let Some(m) = self.mappings[compartment].get(&mapping_id) {
                    self.process_feedback_related_reaper_event_for_mapping(compartment, m, &|_| {
                        (true, None)
                    });
                }
            }
        }
    }

    fn basic_io_changed_event(&self) -> IoUpdatedEvent {
        let active = self.mappings[MappingCompartment::MainMappings]
            .values()
            .any(|m| m.is_effectively_on());
        IoUpdatedEvent {
            instance_id: self.instance_id.clone(),
            control_input: self.control_input.device_input(),
            control_input_used: self.control_is_globally_enabled && active,
            feedback_output: self.feedback_output.and_then(|o| o.device_output()),
            feedback_output_used: self.feedback_is_globally_enabled && active,
            feedback_output_usage_might_have_changed: false,
        }
    }

    fn control_is_effectively_enabled(&self) -> bool {
        self.control_is_globally_enabled
            && BackboneState::get().control_is_allowed(self.instance_id(), self.control_input)
    }

    fn feedback_is_effectively_enabled(&self) -> bool {
        feedback_is_effectively_enabled(
            self.feedback_is_globally_enabled,
            self.instance_id(),
            self.feedback_output,
        )
    }

    fn io_released_event(&self) -> IoUpdatedEvent {
        IoUpdatedEvent {
            control_input_used: false,
            feedback_output_used: false,
            ..self.feedback_output_usage_might_have_changed_event()
        }
    }

    fn feedback_output_usage_might_have_changed_event(&self) -> IoUpdatedEvent {
        IoUpdatedEvent {
            feedback_output_usage_might_have_changed: true,
            ..self.basic_io_changed_event()
        }
    }

    fn notify_feedback_dev_usage_might_have_changed(&self, compartment: MappingCompartment) {
        // A device is only considered to be "in use" if there's at least one
        // *main* mapping. It doesn't depend on
        // controller mappings.
        if compartment == MappingCompartment::MainMappings {
            let event = self.feedback_output_usage_might_have_changed_event();
            debug!(
                self.logger,
                "IO event. Feedback output used: {:?}", event.feedback_output_used
            );
            self.send_io_update(event).unwrap();
        }
    }

    fn send_io_update(
        &self,
        event: IoUpdatedEvent,
    ) -> Result<(), crossbeam_channel::SendError<InstanceOrchestrationEvent>> {
        self.instance_orchestration_event_sender
            .send(InstanceOrchestrationEvent::IoUpdated(event))
    }

    fn get_normal_or_virtual_target_mapping(
        &self,
        compartment: MappingCompartment,
        id: MappingId,
    ) -> Option<&MainMapping> {
        self.mappings[compartment].get(&id).or(
            if compartment == MappingCompartment::ControllerMappings {
                self.mappings_with_virtual_targets.get(&id)
            } else {
                None
            },
        )
    }

    pub fn process_additional_feedback_event(&self, event: &AdditionalFeedbackEvent) {
        if let AdditionalFeedbackEvent::PlayPositionChanged(_) = event {
            // This is fired very frequently so we don't want to iterate over all mappings,
            // just the ones that need to be notified for feedback or whatever.
            for compartment in MappingCompartment::enum_iter() {
                for mapping_id in self.beat_dependent_feedback_mappings[compartment].iter() {
                    if let Some(m) = self.mappings[compartment].get(&mapping_id) {
                        self.process_feedback_related_reaper_event_for_mapping(
                            compartment,
                            m,
                            &|target| target.value_changed_from_additional_feedback_event(event),
                        );
                    }
                }
            }
        } else {
            // Okay, not fired that frequently, we can iterate over all mappings.
            self.process_feedback_related_reaper_event(|target| {
                target.value_changed_from_additional_feedback_event(event)
            });
        }
    }

    pub fn process_control_surface_change_event(&self, event: &ChangeEvent) {
        if ReaperTarget::is_potential_static_change_event(event)
            || ReaperTarget::is_potential_dynamic_change_event(event)
        {
            // Handle dynamic target changes and target activation depending on REAPER state.
            //
            // Whenever anything changes that just affects the main processor targets, resync all
            // targets to the main processor. We don't want to resync to the real-time processor
            // just because another track has been selected. First, it would reset any source state
            // (e.g. short/long press timers). Second, it wouldn't change anything about the
            // sources. We also don't want to resync modes to the main processor. First,
            // it would reset any mode state (e.g. throttling data). Second, it would -
            // again - not result in any change. There are several global conditions
            // which affect whether feedback will be sent from a target or not. Similar
            // global conditions decide what exactly produces the feedback values (e.g.
            // when there's a target which uses <Selected track>, then a track selection
            // change changes the feedback value producer).

            // We don't have mutable access to self here (for good reentrancy reasons) so we
            // do the refresh in the next main loop cycle. This is what we always did, also when
            // this was still based on Rx!
            self.self_normal_sender
                .try_send(NormalMainTask::RefreshAllTargets)
                .unwrap();
        }
        self.process_feedback_related_reaper_event(|target| {
            target.value_changed_from_change_event(event)
        });
    }

    /// The given function should return if the current target value is affected by this change
    /// and - if possible - the new value. We do this because querying the value *immediately*
    /// using the target's `current_value()` method will in some or even many (?) cases give us the
    /// old value - which can lead to confusing feedback! In the past we unknowingly worked around
    /// this by deferring the value query to the next main cycle, but now that we have the nice
    /// non-rx change detection technique, we can do it right here, feedback without delay and
    /// avoid a redundant query.
    fn process_feedback_related_reaper_event(
        &self,
        f: impl Fn(&ReaperTarget) -> (bool, Option<UnitValue>),
    ) {
        for compartment in MappingCompartment::enum_iter() {
            // Mappings with virtual targets don't need to be considered here because they don't
            // cause feedback themselves.
            for m in self.mappings[compartment].values() {
                self.process_feedback_related_reaper_event_for_mapping(compartment, m, &f);
            }
        }
    }

    fn process_feedback_related_reaper_event_for_mapping(
        &self,
        compartment: MappingCompartment,
        m: &MainMapping,
        f: &impl Fn(&ReaperTarget) -> (bool, Option<UnitValue>),
    ) {
        let feedback_is_effectively_on = m.feedback_is_effectively_on();
        let projection_feedback_desired = feedback_is_effectively_on;
        let source_feedback_desired =
            self.feedback_is_effectively_enabled() && feedback_is_effectively_on && !m.is_echo();
        let compound_target = m.target();
        if let Some(CompoundMappingTarget::Reaper(target)) = compound_target {
            let (value_changed, new_value) = f(target);
            if value_changed {
                // Immediate value capturing. Makes OSC feedback *much* smoother in
                // combination with high-throughput thread. Especially quick pulls
                // of many faders at once profit from it because intermediate
                // values are be captured and immediately sent so user doesn't see
                // stuttering faders on their device.
                // It's important to capture the current value from the event because
                // querying *at this time* from the target itself might result in
                // the old value to be returned. This is the case with FX parameter
                // changes for examples and especially in case of on/off targets this
                // can lead to horribly wrong feedback. Previously we didn't have this
                // issue because we always deferred to the next main loop cycle.
                let new_target_value = m
                    .given_or_current_value(new_value, target)
                    .unwrap_or(UnitValue::MIN);
                // Feedback
                let feedback_value = m.feedback_given_target_value(
                    new_target_value,
                    projection_feedback_desired,
                    source_feedback_desired,
                );
                self.send_feedback(FeedbackReason::Normal, feedback_value);
                // Inform session, e.g. for UI updates
                self.event_handler
                    .handle_event(DomainEvent::TargetValueChanged(TargetValueChangedEvent {
                        compartment,
                        mapping_id: m.id(),
                        target: compound_target,
                        new_value: new_target_value,
                    }));
            }
        }
    }

    pub fn notify_target_touched(&self) {
        self.self_feedback_sender
            .try_send(FeedbackMainTask::TargetTouched)
            .unwrap();
    }

    pub fn receives_osc_from(&self, device_id: &OscDeviceId) -> bool {
        self.control_input == ControlInput::Osc(*device_id)
    }

    pub fn process_incoming_osc_packet(&mut self, packet: &OscPacket) {
        match packet {
            OscPacket::Message(msg) => self.process_incoming_osc_message(msg),
            OscPacket::Bundle(bundle) => {
                for p in bundle.content.iter() {
                    self.process_incoming_osc_packet(p);
                }
            }
        }
    }

    fn process_incoming_osc_message(&mut self, msg: &OscMessage) {
        match self.control_mode {
            ControlMode::Controlling => {
                if self.control_is_effectively_enabled() {
                    control_virtual_mappings_osc(
                        &InstanceProps {
                            rt_sender: &self.feedback_real_time_task_sender,
                            fb_audio_hook_task_sender: &self.feedback_audio_hook_task_sender,
                            osc_feedback_task_sender: &self.osc_feedback_task_sender,
                            instance_orchestration_sender: &self
                                .instance_orchestration_event_sender,
                            instance_id: &self.instance_id,
                            feedback_is_globally_enabled: self.feedback_is_globally_enabled,
                            event_handler: &self.event_handler,
                            feedback_output: self.feedback_output,
                            logger: &self.logger,
                        },
                        &mut self.mappings_with_virtual_targets,
                        &mut self.mappings[MappingCompartment::MainMappings],
                        msg,
                    );
                    self.control_non_virtual_mappings_osc(msg);
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                osc_arg_index_hint,
            } => {
                let source = OscSource::from_source_value(msg.clone(), osc_arg_index_hint);
                self.event_handler.handle_event(DomainEvent::LearnedSource {
                    source: RealSource::Osc(source),
                    allow_virtual_sources,
                });
            }
            ControlMode::Disabled => {}
        }
    }

    fn control_non_virtual_mappings_osc(&mut self, msg: &OscMessage) {
        for compartment in MappingCompartment::enum_iter() {
            for m in self.mappings[compartment]
                .values_mut()
                .filter(|m| m.control_is_effectively_on())
            {
                if let CompoundMappingSource::Osc(s) = m.source() {
                    if let Some(control_value) = s.control(msg) {
                        let feedback =
                            m.control_if_enabled(control_value, ControlOptions::default());
                        send_direct_and_virtual_feedback(
                            &InstanceProps {
                                rt_sender: &self.feedback_real_time_task_sender,
                                fb_audio_hook_task_sender: &self.feedback_audio_hook_task_sender,
                                osc_feedback_task_sender: &self.osc_feedback_task_sender,
                                instance_orchestration_sender: &self
                                    .instance_orchestration_event_sender,
                                instance_id: &self.instance_id,
                                feedback_is_globally_enabled: self.feedback_is_globally_enabled,
                                event_handler: &self.event_handler,
                                feedback_output: self.feedback_output,
                                logger: &self.logger,
                            },
                            &self.mappings_with_virtual_targets,
                            FeedbackReason::Normal,
                            feedback,
                        );
                    }
                }
            }
        }
    }

    fn process_mapping_updates_due_to_parameter_changes(
        &mut self,
        compartment: MappingCompartment,
        mapping_activation_updates: Vec<ActivationChange>,
        target_activation_updates: Vec<ActivationChange>,
        unused_sources: &HashSet<QualifiedSource>,
        changed_mappings: impl Iterator<Item = MappingId>,
    ) {
        // Send feedback
        self.handle_feedback_after_having_updated_particular_mappings(
            compartment,
            &unused_sources,
            changed_mappings,
        );
        // Communicate activation changes to real-time processor
        if !mapping_activation_updates.is_empty() {
            self.normal_real_time_task_sender
                .send(NormalRealTimeTask::UpdateMappingActivations(
                    compartment,
                    mapping_activation_updates,
                ))
                .unwrap();
        }
        if !target_activation_updates.is_empty() {
            self.normal_real_time_task_sender
                .send(NormalRealTimeTask::UpdateTargetActivations(
                    compartment,
                    target_activation_updates,
                ))
                .unwrap();
        }
        // Update on mappings
        self.update_on_mappings();
    }

    fn update_on_mappings(&self) {
        let instance_is_enabled =
            self.control_is_effectively_enabled() && self.feedback_is_effectively_enabled();
        let on_mappings = if instance_is_enabled {
            self.all_mappings()
                .filter(|m| m.is_effectively_on())
                .map(MainMapping::id)
                .collect()
        } else {
            HashSet::new()
        };
        self.event_handler
            .handle_event(DomainEvent::UpdatedOnMappings(on_mappings));
    }

    fn send_feedback(
        &self,
        reason: FeedbackReason,
        feedback_values: impl IntoIterator<Item = FeedbackValue>,
    ) {
        send_direct_and_virtual_feedback(
            &self.instance_props(),
            &self.mappings_with_virtual_targets,
            reason,
            feedback_values,
        );
    }

    fn instance_props(&self) -> InstanceProps<EH> {
        InstanceProps {
            rt_sender: &self.feedback_real_time_task_sender,
            fb_audio_hook_task_sender: &self.feedback_audio_hook_task_sender,
            osc_feedback_task_sender: &self.osc_feedback_task_sender,
            instance_orchestration_sender: &self.instance_orchestration_event_sender,
            instance_id: &self.instance_id,
            feedback_is_globally_enabled: self.feedback_is_globally_enabled,
            event_handler: &self.event_handler,
            feedback_output: self.feedback_output,
            logger: &self.logger,
        }
    }

    fn all_mappings(&self) -> impl Iterator<Item = &MainMapping> {
        self.all_mappings_without_virtual_targets()
            .chain(self.mappings_with_virtual_targets.values())
    }

    /// Includes virtual mappings if the controller mapping compartment is queried.
    fn all_mappings_in_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &MainMapping> {
        self.mappings[compartment].values().chain(
            self.mappings_with_virtual_targets
                .values()
                // Include virtual target mappings if we are talking about controller compartment.
                .filter(move |_| compartment == MappingCompartment::ControllerMappings),
        )
    }

    fn all_mappings_without_virtual_targets(&self) -> impl Iterator<Item = &MainMapping> {
        MappingCompartment::enum_iter()
            .map(move |compartment| self.mappings[compartment].values())
            .flatten()
    }

    pub fn send_all_feedback(&self) {
        self.send_feedback(FeedbackReason::Normal, self.feedback_all());
    }

    fn feedback_all(&self) -> Vec<FeedbackValue> {
        // Virtual targets don't cause feedback themselves
        self.all_mappings_without_virtual_targets()
            .filter_map(|m| {
                if m.feedback_is_effectively_on() {
                    m.feedback(true)
                } else {
                    None
                }
            })
            .collect()
    }

    fn feedback_particular_mappings(
        &self,
        compartment: MappingCompartment,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) -> Vec<FeedbackValue> {
        mapping_ids
            .filter_map(|id| {
                let m = self.get_normal_or_virtual_target_mapping(compartment, id)?;
                if m.feedback_is_effectively_on() {
                    self.get_mapping_feedback_follow_virtual(m)
                } else {
                    None
                }
            })
            .collect()
    }

    fn feedback_all_in_compartment(&self, compartment: MappingCompartment) -> Vec<FeedbackValue> {
        self.all_mappings_in_compartment(compartment)
            .filter_map(|m| {
                if m.feedback_is_effectively_on() {
                    self.get_mapping_feedback_follow_virtual(m)
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_mapping_feedback_follow_virtual(&self, m: &MainMapping) -> Option<FeedbackValue> {
        let followed_mapping = self.follow_maybe_virtual_mapping(m)?;
        followed_mapping.feedback(true)
    }

    fn follow_maybe_virtual_mapping<'a>(&'a self, m: &'a MainMapping) -> Option<&'a MainMapping> {
        if let Some(control_element) = m.virtual_target_control_element() {
            self.mappings[MappingCompartment::MainMappings]
                .values()
                .find(|m| {
                    m.virtual_source_control_element() == Some(control_element)
                        && m.feedback_is_effectively_on()
                })
        } else {
            Some(m)
        }
    }

    pub fn handle_change_of_some_upper_floor_instance(
        &self,
        feedback_output: DeviceFeedbackOutput,
    ) {
        if self.feedback_output.and_then(FeedbackOutput::device_output) == Some(feedback_output) {
            if self.feedback_is_effectively_enabled() {
                debug!(self.logger, "Reactivating instance...");
                // For this to really work reliably (eventual feedback consistency), it was
                // necessary to let the direct MIDI device feedback process in the global
                // *audio hook*, not in the real-time processor. Because there's only one audio
                // hook can guarantee a deterministic feedback send order.
                self.send_all_feedback();
            } else {
                debug!(self.logger, "Cancelling instance...");
                self.send_feedback(FeedbackReason::SuspendInstance, self.feedback_all_zero());
            }
        }
        self.update_on_mappings();
    }

    /// When feedback gets globally disabled.
    fn clear_all_feedback_allowing_source_takeover(&self) {
        debug!(
            self.logger,
            "Clearing all feedback allowing source takeover..."
        );
        self.send_feedback(
            FeedbackReason::ClearAllAllowingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    /// When main processor goes away for good.
    fn clear_all_feedback_preventing_source_takeover(&self) {
        debug!(
            self.logger,
            "Clearing all feedback preventing source takeover..."
        );
        self.send_feedback(
            FeedbackReason::ClearAllPreventingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    fn feedback_all_zero(&self) -> Vec<FeedbackValue> {
        // Mappings with virtual targets should not be included here because they might not be in
        // use and therefore should not *directly* send zeros. However, they will receive zeros
        // if one of the main mappings with virtual sources are connected to them.
        self.all_mappings_without_virtual_targets()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| m.zero_feedback())
            .collect()
    }

    fn currently_feedback_enabled_sources(
        &self,
        compartment: MappingCompartment,
        include_virtual: bool,
    ) -> HashSet<QualifiedSource> {
        if include_virtual {
            self.all_mappings_in_compartment(compartment)
                .filter(|m| m.feedback_is_effectively_on())
                .map(MainMapping::qualified_source)
                .collect()
        } else {
            self.mappings[compartment]
                .values()
                .filter(|m| m.feedback_is_effectively_on())
                .map(MainMapping::qualified_source)
                .collect()
        }
    }

    fn handle_feedback_after_having_updated_all_mappings(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: &HashSet<QualifiedSource>,
    ) {
        self.send_zero_feedback_for_unused_sources(now_unused_sources);
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_all_in_compartment(compartment),
        );
    }

    fn handle_feedback_after_having_updated_particular_mappings(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: &HashSet<QualifiedSource>,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) {
        self.send_zero_feedback_for_unused_sources(now_unused_sources);
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_particular_mappings(compartment, mapping_ids),
        );
    }

    /// Indicate via zero feedback the sources which are not in use anymore.
    fn send_zero_feedback_for_unused_sources(&self, now_unused_sources: &HashSet<QualifiedSource>) {
        for s in now_unused_sources {
            self.send_feedback(FeedbackReason::ClearUnusedSource, s.zero_feedback());
        }
    }

    fn log_debug_info(&mut self, task_count: usize) {
        // Summary
        let msg = format!(
            "\n\
            # Main processor\n\
            \n\
            - State: {:?} \n\
            - Total main mapping count: {} \n\
            - Enabled main mapping count: {} \n\
            - Total non-virtual controller mapping count: {} \n\
            - Enabled non-virtual controller mapping count: {} \n\
            - Total virtual controller mapping count: {} \n\
            - Enabled virtual controller mapping count: {} \n\
            - Normal task count: {} \n\
            - Control task count: {} \n\
            - Feedback task count: {} \n\
            - Parameter values: {:?} \n\
            ",
            self.control_mode,
            self.mappings[MappingCompartment::MainMappings].len(),
            self.mappings[MappingCompartment::MainMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.mappings[MappingCompartment::ControllerMappings].len(),
            self.mappings[MappingCompartment::ControllerMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.mappings_with_virtual_targets.len(),
            self.mappings_with_virtual_targets
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
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

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(MappingCompartment, Vec<MainMapping>),
    /// Replaces the given mapping.
    // Boxed because much larger struct size than other variants.
    UpdateSingleMapping(MappingCompartment, Box<MainMapping>),
    RefreshAllTargets,
    UpdateSettings {
        control_input: ControlInput,
        feedback_output: Option<FeedbackOutput>,
    },
    UpdateControlIsGloballyEnabled(bool),
    UpdateFeedbackIsGloballyEnabled(bool),
    SendAllFeedback,
    LogDebugInfo,
    LearnMidiSource {
        source: MidiSource,
        allow_virtual_sources: bool,
    },
    StartLearnSource {
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    },
    DisableControl,
    ReturnToControlMode,
    /// This is sent by the real-time processor after it has not been called for a while because
    /// the audio device was closed. It wants everything resynced:
    ///
    /// - All mappings
    /// - Instance settings
    /// - Feedback
    FullResyncToRealTimeProcessorPlease,
}

/// A parameter-related task (which is potentially sent very frequently, just think of automation).
#[derive(Debug)]
pub enum ParameterMainTask {
    UpdateParameter { index: u32, value: f32 },
    UpdateAllParameters(Box<ParameterArray>),
}

/// A feedback-related task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackMainTask {
    /// Sent whenever a target has been touched (usually a subset of the value change events)
    /// and as a result the global "last touched target" has been updated.
    TargetTouched,
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ControlOptions {
    pub enforce_send_feedback_after_control: bool,
    pub mode_control_options: ModeControlOptions,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping main processor...");
        if self.feedback_is_effectively_enabled() {
            // We clear feedback right here and now because that's the last chance.
            // Other instances can take over the feedback output afterwards.
            self.clear_all_feedback_preventing_source_takeover();
        }
        let _ = self.send_io_update(self.io_released_event());
    }
}

/// Different feedback reasons can but don't have to result in slightly different behavior.
///
/// In any case, they are nice for tracing when debugging feedback issues.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FeedbackReason {
    /// When ReaLearn detects a single source as unused.
    ClearUnusedSource,
    /// When all feedback for that instance gets disabled (e.g. by instance deactivation) but other
    /// instances should get a chance to grab some sources. Must be processed even if feedback
    /// globally disabled (because sent *after* globally disabling feedback).
    ClearAllAllowingSourceTakeover,
    /// When all feedback for that instance gets disabled and switching off is more important than
    /// letting other instances take over (e.g. when removing instance completely). Only needs to
    /// be processed when feedback enabled.
    ClearAllPreventingSourceTakeover,
    /// When a lower-floor ReaLearn instance is cancelled by an upper-floor one. Must be processed
    /// even if feedback is effectively disabled (because sent at a time when the lower-floor
    /// instance is covered by the higher-floor instance already).
    SuspendInstance,
    /// Normal feedback scenarios.
    Normal,
    /// When a ReaLearn instance X takes control of a source after Y has released the source.
    TakeOverSource,
    /// When no ReaLearn instance has taken over a source and now it's finally being switched off.
    FinallySwitchOffSource,
}

impl FeedbackReason {
    /// When this returns true, it allows source takeover by other instances.
    pub fn is_source_release(self) -> bool {
        use FeedbackReason::*;
        matches!(
            self,
            ClearUnusedSource | ClearAllAllowingSourceTakeover | SuspendInstance
        )
    }

    /// When this returns true, the processor will initiate the feedback send logic (including
    /// source takeover) always regardless if feedback is globally enabled or not.
    pub fn is_always_allowed(self) -> bool {
        matches!(
            self,
            FeedbackReason::SuspendInstance | FeedbackReason::ClearAllAllowingSourceTakeover
        )
    }
}

struct InstanceProps<'a, EH: DomainEventHandler> {
    rt_sender: &'a RealTimeSender<FeedbackRealTimeTask>,
    fb_audio_hook_task_sender: &'a RealTimeSender<FeedbackAudioHookTask>,
    osc_feedback_task_sender: &'a crossbeam_channel::Sender<OscFeedbackTask>,
    instance_orchestration_sender: &'a crossbeam_channel::Sender<InstanceOrchestrationEvent>,
    instance_id: &'a str,
    feedback_is_globally_enabled: bool,
    event_handler: &'a EH,
    feedback_output: Option<FeedbackOutput>,
    logger: &'a slog::Logger,
}

impl<'a, EH: DomainEventHandler> InstanceProps<'a, EH> {
    pub fn feedback_is_effectively_enabled(&self) -> bool {
        feedback_is_effectively_enabled(
            self.feedback_is_globally_enabled,
            self.instance_id,
            self.feedback_output,
        )
    }
}

/// Sends both direct and virtual-source feedback.
fn send_direct_and_virtual_feedback<EH: DomainEventHandler>(
    instance: &InstanceProps<EH>,
    mappings_with_virtual_targets: &HashMap<MappingId, MainMapping>,
    feedback_reason: FeedbackReason,
    feedback_values: impl IntoIterator<Item = FeedbackValue>,
) {
    for feedback_value in feedback_values.into_iter() {
        match feedback_value {
            FeedbackValue::Virtual {
                with_projection_feedback,
                with_source_feedback,
                value,
            } => {
                if let ControlValue::Absolute(v) = value.control_value() {
                    for m in mappings_with_virtual_targets
                        .values()
                        .filter(|m| m.feedback_is_effectively_on())
                    {
                        if let Some(CompoundMappingTarget::Virtual(t)) = m.target() {
                            if t.control_element() == value.control_element() {
                                if let Some(FeedbackValue::Real(final_feedback_value)) = m
                                    .feedback_given_target_value(
                                        v,
                                        with_projection_feedback,
                                        with_source_feedback,
                                    )
                                {
                                    send_direct_feedback(
                                        instance,
                                        feedback_reason,
                                        final_feedback_value,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            FeedbackValue::Real(final_feedback_value) => {
                send_direct_feedback(instance, feedback_reason, final_feedback_value);
            }
        }
    }
}

fn send_direct_feedback<EH: DomainEventHandler>(
    instance: &InstanceProps<EH>,
    feedback_reason: FeedbackReason,
    feedback_value: RealFeedbackValue,
) {
    if feedback_reason.is_always_allowed() || instance.feedback_is_effectively_enabled() {
        if let Some(feedback_output) = instance.feedback_output {
            if let Some(source_feedback_value) = feedback_value.source {
                // At this point we can be sure that this mapping can't have a
                // virtual source.
                if feedback_reason.is_source_release() {
                    // Possible interference with other instances. Don't switch off yet!
                    // Give other instances the chance to take over.
                    let event = InstanceOrchestrationEvent::SourceReleased(SourceReleasedEvent {
                        instance_id: instance.instance_id.to_owned(),
                        feedback_output,
                        feedback_value: source_feedback_value,
                    });
                    instance.instance_orchestration_sender.send(event).unwrap();
                } else {
                    // Send feedback right now.
                    send_direct_source_feedback(
                        instance,
                        feedback_output,
                        feedback_reason,
                        source_feedback_value,
                    );
                }
            }
        }
    }
    if let Some(projection_feedback_value) = feedback_value.projection {
        instance
            .event_handler
            .handle_event(DomainEvent::ProjectionFeedback(projection_feedback_value));
    }
}

fn send_direct_source_feedback<EH: DomainEventHandler>(
    instance: &InstanceProps<EH>,
    feedback_output: FeedbackOutput,
    feedback_reason: FeedbackReason,
    source_feedback_value: SourceFeedbackValue,
) {
    // No interference with other instances.
    debug!(
        instance.logger,
        "Schedule sending feedback because {:?}: {:?}", feedback_reason, source_feedback_value
    );
    match source_feedback_value {
        SourceFeedbackValue::Midi(v) => {
            if let FeedbackOutput::Midi(midi_output) = feedback_output {
                match midi_output {
                    MidiDestination::FxOutput => {
                        instance
                            .rt_sender
                            .send(FeedbackRealTimeTask::FxOutputFeedback(v))
                            .unwrap();
                    }
                    MidiDestination::Device(dev_id) => {
                        // We send to the audio hook in this case (the default case) because there's
                        // only one audio hook (not one per instance as with real-time processors),
                        // so it can guarantee us a globally deterministic order. This is necessary
                        // to achieve "eventual feedback consistency" by using instance
                        // orchestration techniques in the main thread. If
                        // we don't do that, we can prepare the most perfect
                        // feedback ordering in the backbone control surface (main
                        // thread, in order to support multiple instances with the same device) ...
                        // it won't be useful at all if the real-time processors send the feedback
                        // in the order of instance instantiation.
                        instance
                            .fb_audio_hook_task_sender
                            .send(FeedbackAudioHookTask::MidiDeviceFeedback(dev_id, v))
                            .unwrap();
                    }
                }
            }
        }
        SourceFeedbackValue::Osc(msg) => {
            if let FeedbackOutput::Osc(dev_id) = feedback_output {
                instance
                    .osc_feedback_task_sender
                    .try_send(OscFeedbackTask::new(dev_id, msg))
                    .unwrap();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn control_virtual_mappings_osc<EH: DomainEventHandler>(
    instance: &InstanceProps<EH>,
    mappings_with_virtual_targets: &mut HashMap<MappingId, MainMapping>,
    // Contains mappings with virtual sources
    main_mappings: &mut HashMap<MappingId, MainMapping>,
    msg: &OscMessage,
) {
    // Control
    let source_values: Vec<_> = mappings_with_virtual_targets
        .values_mut()
        .filter(|m| m.control_is_effectively_on())
        .flat_map(|m| {
            if let Some(control_match) = m.control_osc_virtualizing(msg) {
                use PartialControlMatch::*;
                match control_match {
                    ProcessVirtual(virtual_source_value) => {
                        control_main_mappings_virtual(
                            main_mappings,
                            virtual_source_value,
                            ControlOptions {
                                // We inherit "Send feedback after control" if it's
                                // enabled for the virtual mapping. That's the easy way to do it.
                                // Downside: If multiple real control elements are mapped to one
                                // virtual control element,
                                // "feedback after control" will be sent to all of
                                // those, which is technically not
                                // necessary. It would be enough to just send it
                                // to the one that was touched. However, it also doesn't really
                                // hurt.
                                enforce_send_feedback_after_control: m
                                    .options()
                                    .send_feedback_after_control,
                                mode_control_options: m.mode_control_options(),
                            },
                        )
                    }
                    ProcessDirect(_) => {
                        unreachable!("we shouldn't be here")
                    }
                }
            } else {
                vec![]
            }
        })
        .collect();
    // Feedback
    send_direct_and_virtual_feedback(
        instance,
        mappings_with_virtual_targets,
        FeedbackReason::Normal,
        source_values,
    );
}

fn control_main_mappings_virtual(
    main_mappings: &mut HashMap<MappingId, MainMapping>,
    value: VirtualSourceValue,
    options: ControlOptions,
) -> Vec<FeedbackValue> {
    // Controller mappings can't have virtual sources, so for now we only need to check
    // main mappings.
    main_mappings
        .values_mut()
        .filter(|m| m.control_is_effectively_on())
        .filter_map(|m| {
            if let CompoundMappingSource::Virtual(s) = &m.source() {
                let control_value = s.control(&value)?;
                m.control_if_enabled(control_value, options)
            } else {
                None
            }
        })
        .collect()
}

/// Includes virtual mappings if the controller mapping compartment is queried.
fn all_mappings_in_compartment_mut<'a>(
    mappings: &'a mut EnumMap<MappingCompartment, HashMap<MappingId, MainMapping>>,
    mappings_with_virtual_targets: &'a mut HashMap<MappingId, MainMapping>,
    compartment: MappingCompartment,
) -> impl Iterator<Item = &'a mut MainMapping> {
    mappings[compartment].values_mut().chain(
        mappings_with_virtual_targets
            .values_mut()
            // Include virtual target mappings if we are talking about controller compartment.
            .filter(move |_| compartment == MappingCompartment::ControllerMappings),
    )
}

fn get_normal_or_virtual_target_mapping_mut<'a>(
    mappings: &'a mut EnumMap<MappingCompartment, HashMap<MappingId, MainMapping>>,
    mappings_with_virtual_targets: &'a mut HashMap<MappingId, MainMapping>,
    compartment: MappingCompartment,
    id: MappingId,
) -> Option<&'a mut MainMapping> {
    mappings[compartment].get_mut(&id).or(
        if compartment == MappingCompartment::ControllerMappings {
            mappings_with_virtual_targets.get_mut(&id)
        } else {
            None
        },
    )
}

fn feedback_is_effectively_enabled(
    feedback_is_globally_enabled: bool,
    instance_id: &str,
    feedback_output: Option<FeedbackOutput>,
) -> bool {
    if let Some(fo) = feedback_output {
        feedback_is_globally_enabled && BackboneState::get().feedback_is_allowed(instance_id, fo)
    } else {
        // Pointless but allowed
        true
    }
}
