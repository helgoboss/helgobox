use crate::domain::{
    AdditionalFeedbackEvent, CompoundMappingSource, CompoundMappingTarget, ControlMode,
    DomainEvent, DomainEventHandler, FeedbackRealTimeTask, MainMapping, MappingActivationEffect,
    MappingActivationUpdate, MappingCompartment, MappingId, NormalRealTimeTask, OscDeviceId,
    OscFeedbackTask, PartialControlMatch, ProcessorContext, RealSource,
    RealearnMonitoringFxParameterValueChangedEvent, ReaperTarget, SourceValue,
    TargetValueChangedEvent, VirtualSourceValue,
};
use enum_iterator::IntoEnumIterator;
use enum_map::EnumMap;
use helgoboss_learn::{ControlValue, MidiSource, OscSource, UnitValue};

use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::ReaperNormalizedFxParamValue;
use rosc::{OscMessage, OscPacket};
use slog::debug;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

const NORMAL_TASK_BULK_SIZE: usize = 32;
const FEEDBACK_TASK_BULK_SIZE: usize = 64;
const CONTROL_TASK_BULK_SIZE: usize = 32;
const PARAMETER_TASK_BULK_SIZE: usize = 32;

// TODO-low Making this a usize might save quite some code
pub const PLUGIN_PARAMETER_COUNT: u32 = 100;
pub type ParameterArray = [f32; PLUGIN_PARAMETER_COUNT as usize];
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
    feedback_is_globally_enabled: bool,
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
    feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
    additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    parameters: ParameterArray,
    event_handler: EH,
    context: ProcessorContext,
    control_mode: ControlMode,
    control_is_globally_enabled: bool,
    osc_input_device_id: Option<OscDeviceId>,
    osc_output_device_id: Option<OscDeviceId>,
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
        normal_real_time_task_sender: crossbeam_channel::Sender<NormalRealTimeTask>,
        feedback_real_time_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
        osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
        event_handler: EH,
        context: ProcessorContext,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) = crossbeam_channel::unbounded();
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
            feedback_is_globally_enabled: false,
            parameters: ZEROED_PLUGIN_PARAMETERS,
            event_handler,
            context,
            control_mode: ControlMode::Controlling,
            control_is_globally_enabled: true,
            osc_input_device_id: None,
            osc_output_device_id: None,
            osc_feedback_task_sender,
            additional_feedback_event_sender,
        }
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
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
                    // Resolving mappings with virtual targets is not necessary anymore. It has
                    // been done in the real-time processor already.
                    if let Some(m) = self.mappings[compartment].get_mut(&mapping_id) {
                        control_and_optionally_feedback(
                            &self.feedback_real_time_task_sender,
                            &self.osc_feedback_task_sender,
                            m,
                            value,
                            options,
                            self.osc_output_device_id.as_ref(),
                            &self.mappings_with_virtual_targets,
                        );
                    };
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
                    osc_input_device_id,
                    osc_output_device_id,
                } => {
                    self.osc_input_device_id = osc_input_device_id;
                    self.osc_output_device_id = osc_output_device_id;
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
                    // Refresh and splinter real-time mappings
                    let real_time_mappings = mappings
                        .iter_mut()
                        .map(|m| {
                            m.refresh_all(&self.context, &self.parameters);
                            if m.feedback_is_effectively_on() {
                                // Mark source as used
                                unused_sources.remove(m.source());
                            }
                            if m.needs_refresh_when_target_touched() {
                                self.target_touch_dependent_mappings[compartment].insert(m.id());
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
                    self.handle_feedback_after_batch_mapping_update(compartment, &unused_sources);
                    self.update_on_mappings();
                }
                RefreshAllTargets => {
                    debug!(self.logger, "Refreshing all targets...");
                    for compartment in MappingCompartment::into_enum_iter() {
                        let mut unused_sources =
                            self.currently_feedback_enabled_sources(compartment, false);
                        let mut mappings_with_active_targets =
                            HashSet::with_capacity(self.mappings[compartment].len());
                        // Mappings with virtual targets don't have to be refreshed because virtual
                        // targets are always active and never change depending on circumstances.
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
                        // In some cases like closing projects, it's possible that this will fail
                        // because the real-time processor is already gone. But it doesn't matter.
                        let _ = self.normal_real_time_task_sender.send(
                            NormalRealTimeTask::UpdateTargetActivations(
                                compartment,
                                mappings_with_active_targets,
                            ),
                        );
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
                            Box::new(mapping.splinter_real_time_mapping()),
                        ))
                        .unwrap();
                    // (Re)subscribe to or unsubscribe from feedback
                    if self.feedback_is_globally_enabled {
                        match mapping.target() {
                            Some(CompoundMappingTarget::Reaper(_))
                                if mapping.feedback_is_effectively_on() =>
                            {
                                self.send_feedback(mapping.feedback_if_enabled());
                            }
                            _ => {
                                // Indicate via feedback that this source is not in use anymore. But
                                // only if feedback was enabled before (otherwise this could
                                // overwrite the feedback value of another enabled mapping which has
                                // the same source).
                                let was_previously_enabled = self.mappings[compartment]
                                    .get(&mapping.id())
                                    // Mappings with virtual targets can also get feedback-disabled.
                                    .or(if compartment == MappingCompartment::ControllerMappings {
                                        self.mappings_with_virtual_targets.get(&mapping.id())
                                    } else {
                                        None
                                    })
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
                    if mapping.needs_refresh_when_target_touched() {
                        self.target_touch_dependent_mappings[compartment].insert(mapping.id());
                    } else {
                        self.target_touch_dependent_mappings[compartment].remove(&mapping.id());
                    }
                    let relevant_map = if mapping.has_virtual_target() {
                        self.mappings[compartment].remove(&mapping.id());
                        &mut self.mappings_with_virtual_targets
                    } else {
                        self.mappings_with_virtual_targets.remove(&mapping.id());
                        &mut self.mappings[compartment]
                    };
                    relevant_map.insert(mapping.id(), *mapping);
                    // TODO-low Mmh, iterating over all mappings might be a bit overkill here.
                    self.update_on_mappings();
                }
                FeedbackAll => {
                    self.send_bulk_feedback();
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
                    self.feedback_is_globally_enabled = is_enabled;
                    if is_enabled {
                        for compartment in MappingCompartment::into_enum_iter() {
                            self.handle_feedback_after_batch_mapping_update(
                                compartment,
                                &HashSet::new(),
                            );
                        }
                    } else {
                        self.clear_feedback();
                    }
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
                    // Activation is only supported for main mappings
                    let compartment = MappingCompartment::MainMappings;
                    let mut activation_updates: Vec<MappingActivationUpdate> = vec![];
                    let mut unused_sources =
                        self.currently_feedback_enabled_sources(compartment, false);
                    // Mappings with virtual targets can only exist in the controller compartment
                    // and the mappings in there don't support conditional activation, so we don't
                    // need to handle them here.
                    for m in &mut self.mappings[compartment].values_mut() {
                        if m.can_be_affected_by_parameters() {
                            m.refresh_activation(&self.parameters);
                            let update = MappingActivationUpdate::new(m.id(), m.is_active());
                            activation_updates.push(update);
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
                    // Work around REAPER's inability to notify about parameter changes in
                    // monitoring FX by simulating the notification ourselves.
                    // Then parameter learning and feedback works at least for
                    // ReaLearn monitoring FX instances, which is especially
                    // useful for conditional activation.
                    if self.context.is_on_monitoring_fx_chain() {
                        let parameter = self.context.containing_fx().parameter_by_index(index);
                        self.additional_feedback_event_sender
                            .send(
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
                    // Activation is only supported for main mappings
                    let compartment = MappingCompartment::MainMappings;
                    let mut unused_sources =
                        self.currently_feedback_enabled_sources(compartment, false);
                    // In order to avoid a mutable borrow of mappings and an immutable borrow of
                    // parameters at the same time, we need to separate into READ activation
                    // affects and WRITE activation updates.
                    // 1. Read
                    let activation_effects: Vec<MappingActivationEffect> = self.mappings
                        [compartment]
                        .values()
                        .filter_map(|m| {
                            m.check_activation_effect(&self.parameters, index, previous_value)
                        })
                        .collect();
                    // 2. Write
                    let activation_updates: Vec<MappingActivationUpdate> = activation_effects
                        .into_iter()
                        .filter_map(|eff| {
                            let m = self.mappings[compartment].get_mut(&eff.id)?;
                            m.update_activation(eff)
                        })
                        .collect();
                    // Determine unused sources
                    for m in self.mappings[compartment].values() {
                        if m.feedback_is_effectively_on() {
                            // Mark source as used
                            unused_sources.remove(m.source());
                        }
                    }
                    self.process_activation_updates(
                        compartment,
                        activation_updates,
                        &unused_sources,
                    )
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
                    for compartment in MappingCompartment::into_enum_iter() {
                        for mapping_id in self.target_touch_dependent_mappings[compartment].iter() {
                            // Virtual targets are not candidates for "Last touched" so we don't
                            // need to consider them here.
                            if let Some(m) = self.mappings[compartment].get_mut(&mapping_id) {
                                m.refresh_target(&self.context);
                                // Switching off shouldn't be necessary since the last touched
                                // target can never be "unset".
                                if self.feedback_is_globally_enabled
                                    && m.feedback_is_effectively_on()
                                {
                                    if let Some(CompoundMappingTarget::Reaper(_)) = m.target() {
                                        send_feedback_direct_virtual(
                                            &self.feedback_real_time_task_sender,
                                            &self.osc_feedback_task_sender,
                                            m.feedback_if_enabled(),
                                            self.osc_output_device_id.as_ref(),
                                            &self.mappings_with_virtual_targets,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn process_additional_feedback_event(&self, event: &AdditionalFeedbackEvent) {
        self.process_feedback_related_reaper_event(|target| {
            target.value_changed_from_additional_feedback_event(event)
        });
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
                .send(NormalMainTask::RefreshAllTargets)
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
        if !self.feedback_is_globally_enabled {
            return;
        }
        for compartment in MappingCompartment::into_enum_iter() {
            // Mappings with virtual targets don't need to be considered here because they don't
            // cause feedback themselves.
            for m in self.mappings[compartment].values() {
                if m.feedback_is_effectively_on() && !m.is_echo() {
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
                            let new_value = m
                                .given_or_current_value(new_value, target)
                                .unwrap_or(UnitValue::MIN);
                            // Feedback
                            let source_value = m.feedback_given_value(new_value);
                            self.send_feedback(source_value);
                            // Inform session, e.g. for UI updates
                            self.event_handler
                                .handle_event(DomainEvent::TargetValueChanged(
                                    TargetValueChangedEvent {
                                        compartment,
                                        mapping_id: m.id(),
                                        target: compound_target,
                                        new_value,
                                    },
                                ));
                        }
                    }
                }
            }
        }
    }

    pub fn notify_target_touched(&self) {
        self.self_feedback_sender
            .send(FeedbackMainTask::TargetTouched)
            .unwrap();
    }

    pub fn receives_osc_from(&self, device_id: &OscDeviceId) -> bool {
        self.osc_input_device_id.contains(device_id)
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
                if self.control_is_globally_enabled {
                    control_virtual_mappings_osc(
                        &self.feedback_real_time_task_sender,
                        &self.osc_feedback_task_sender,
                        &mut self.mappings_with_virtual_targets,
                        &mut self.mappings[MappingCompartment::MainMappings],
                        msg,
                        self.osc_output_device_id.as_ref(),
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
        for compartment in MappingCompartment::into_enum_iter() {
            for mut m in self.mappings[compartment]
                .values_mut()
                .filter(|m| m.control_is_effectively_on())
            {
                if let CompoundMappingSource::Osc(s) = m.source() {
                    if let Some(control_value) = s.control(msg) {
                        control_and_optionally_feedback(
                            &self.feedback_real_time_task_sender,
                            &self.osc_feedback_task_sender,
                            &mut m,
                            control_value,
                            ControlOptions {
                                enforce_send_feedback_after_control: false,
                            },
                            self.osc_output_device_id.as_ref(),
                            &self.mappings_with_virtual_targets,
                        );
                    }
                }
            }
        }
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
            .handle_event(DomainEvent::UpdatedOnMappings(on_mappings));
    }

    fn send_feedback(&self, source_values: impl IntoIterator<Item = SourceValue>) {
        send_feedback_direct_virtual(
            &self.feedback_real_time_task_sender,
            &self.osc_feedback_task_sender,
            source_values,
            self.osc_output_device_id.as_ref(),
            &self.mappings_with_virtual_targets,
        );
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
        MappingCompartment::into_enum_iter()
            .map(move |compartment| self.mappings[compartment].values())
            .flatten()
    }

    pub fn send_bulk_feedback(&self) {
        if self.feedback_is_globally_enabled {
            self.send_feedback(self.feedback_all());
        }
    }

    fn feedback_all(&self) -> Vec<SourceValue> {
        // Virtual targets don't cause feedback themselves
        self.all_mappings_without_virtual_targets()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn feedback_all_in_compartment(&self, compartment: MappingCompartment) -> Vec<SourceValue> {
        // Virtual targets don't deliver feedback, so no need to handle them.
        self.mappings[compartment]
            .values()
            .filter_map(|m| m.feedback_if_enabled())
            .collect()
    }

    fn clear_feedback(&self) {
        if self.osc_output_device_id.is_some() {
            self.send_feedback(self.feedback_all_zero());
        } else {
            self.feedback_real_time_task_sender
                .send(FeedbackRealTimeTask::ClearFeedback)
                .unwrap();
        }
    }

    fn feedback_all_zero(&self) -> Vec<SourceValue> {
        // Mappings with virtual targets should not be included here because they might not be in
        // use and therefore should not *directly* send zeros. However, they will receive zeros
        // if one of the main mappings with virtual sources are connected to them.
        self.all_mappings_without_virtual_targets()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| m.source().feedback(UnitValue::MIN))
            .collect()
    }

    fn currently_feedback_enabled_sources(
        &self,
        compartment: MappingCompartment,
        include_virtual: bool,
    ) -> HashSet<CompoundMappingSource> {
        if include_virtual {
            self.all_mappings_in_compartment(compartment)
                .filter(|m| m.feedback_is_effectively_on())
                .map(|m| m.source().clone())
                .collect()
        } else {
            self.mappings[compartment]
                .values()
                .filter(|m| m.feedback_is_effectively_on())
                .map(|m| m.source().clone())
                .collect()
        }
    }

    fn handle_feedback_after_batch_mapping_update(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: &HashSet<CompoundMappingSource>,
    ) {
        if !self.feedback_is_globally_enabled {
            return;
        }
        // Send feedback instantly to reflect this change in mappings.
        // At first indicate via feedback the sources which are not in use anymore.
        for s in now_unused_sources {
            self.send_feedback(s.feedback(UnitValue::MIN));
        }
        self.send_feedback(self.feedback_all_in_compartment(compartment));
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
        osc_input_device_id: Option<OscDeviceId>,
        osc_output_device_id: Option<OscDeviceId>,
    },
    UpdateFeedbackIsGloballyEnabled(bool),
    FeedbackAll,
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
    UpdateControlIsGloballyEnabled(bool),
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ControlOptions {
    pub enforce_send_feedback_after_control: bool,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping main processor...");
        if self.feedback_is_globally_enabled {
            self.clear_feedback();
        }
    }
}

fn control_and_optionally_feedback(
    rt_sender: &crossbeam_channel::Sender<FeedbackRealTimeTask>,
    osc_feedback_task_sender: &crossbeam_channel::Sender<OscFeedbackTask>,
    mapping: &mut MainMapping,
    value: ControlValue,
    options: ControlOptions,
    osc_device_id: Option<&OscDeviceId>,
    mappings_with_virtual_targets: &HashMap<MappingId, MainMapping>,
) {
    // Most of the time, the main processor won't even receive a MIDI-triggered control
    // instruction from the real-time processor for a mapping for which
    // control is disabled, because the real-time processor doesn't process
    // disabled mappings. But if control is (temporarily) disabled because a
    // target condition is (temporarily) not met (e.g. "track must be
    // selected") and the real-time processor doesn't yet know about it, there
    // might be a short amount of time where we still receive control
    // statements. We filter them here.
    let feedback = mapping.control_if_enabled(value, options);
    send_feedback_direct_virtual(
        rt_sender,
        osc_feedback_task_sender,
        feedback,
        osc_device_id,
        mappings_with_virtual_targets,
    );
}

fn send_feedback_direct_virtual(
    rt_sender: &crossbeam_channel::Sender<FeedbackRealTimeTask>,
    osc_feedback_task_sender: &crossbeam_channel::Sender<OscFeedbackTask>,
    source_values: impl IntoIterator<Item = SourceValue>,
    osc_device_id: Option<&OscDeviceId>,
    mappings_with_virtual_targets: &HashMap<MappingId, MainMapping>,
) {
    for source_value in source_values.into_iter() {
        use SourceValue::*;
        match source_value {
            Osc(msg) => {
                if let Some(id) = osc_device_id {
                    osc_feedback_task_sender
                        .send(OscFeedbackTask::new(*id, msg))
                        .unwrap();
                }
            }
            Midi(v) => {
                rt_sender.send(FeedbackRealTimeTask::Feedback(v)).unwrap();
            }
            Virtual(virtual_source_value) => {
                if let ControlValue::Absolute(v) = virtual_source_value.control_value() {
                    for m in mappings_with_virtual_targets
                        .values()
                        .filter(|m| m.feedback_is_effectively_on())
                    {
                        if let Some(CompoundMappingTarget::Virtual(t)) = m.target() {
                            if t.control_element() == virtual_source_value.control_element() {
                                if let Some(source_value) = m.feedback_given_value(v) {
                                    match source_value {
                                        Midi(v) => {
                                            // TODO-low Maybe we should use the SmallVec here, too?
                                            rt_sender
                                                .send(FeedbackRealTimeTask::Feedback(v))
                                                .unwrap();
                                        }
                                        Osc(msg) => {
                                            if let Some(osc_device_id) = osc_device_id {
                                                osc_feedback_task_sender
                                                    .send(OscFeedbackTask::new(*osc_device_id, msg))
                                                    .unwrap();
                                            }
                                        }
                                        Virtual(_) => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn control_virtual_mappings_osc(
    rt_sender: &crossbeam_channel::Sender<FeedbackRealTimeTask>,
    osc_feedback_task_sender: &crossbeam_channel::Sender<OscFeedbackTask>,
    mappings_with_virtual_targets: &mut HashMap<MappingId, MainMapping>,
    // Contains mappings with virtual sources
    main_mappings: &mut HashMap<MappingId, MainMapping>,
    msg: &OscMessage,
    osc_device_id: Option<&OscDeviceId>,
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
    send_feedback_direct_virtual(
        rt_sender,
        osc_feedback_task_sender,
        source_values,
        osc_device_id,
        mappings_with_virtual_targets,
    );
}

fn control_main_mappings_virtual(
    main_mappings: &mut HashMap<MappingId, MainMapping>,
    value: VirtualSourceValue,
    options: ControlOptions,
) -> Vec<SourceValue> {
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
