use crate::domain::{
    aggregate_target_values, format_as_pretty_hex, get_project_options, say,
    AdditionalFeedbackEvent, AdditionalLuaFeedbackScriptInput, AdditionalLuaMidiSourceScriptInput,
    Backbone, CompartmentKind, CompoundChangeEvent, CompoundFeedbackValue, CompoundMappingSource,
    CompoundMappingSourceAddress, CompoundMappingTarget, ControlContext, ControlEvent,
    ControlEventTimestamp, ControlInput, ControlLogContext, ControlLogEntry, ControlLogEntryKind,
    ControlMode, ControlOutcome, DeviceFeedbackOutput, DomainEvent, DomainEventHandler,
    ExtendedProcessorContext, FeedbackAudioHookTask, FeedbackCause, FeedbackCollector,
    FeedbackDestinations, FeedbackLogEntry, FeedbackOutput, FeedbackRealTimeTask,
    FeedbackResolution, FeedbackSendBehavior, FinalRealFeedbackValue, FinalSourceFeedbackValue,
    GlobalControlAndFeedbackState, GroupId, HitInstructionContext, HitInstructionResponse,
    InstanceId, InternalInfoEvent, IoUpdatedEvent, KeyMessage, MainMapping, MainSourceMessage,
    MappingActivationEffect, MappingControlResult, MappingId, MappingInfo, MessageCaptureEvent,
    MessageCaptureResult, MidiControlInput, MidiDestination, MidiScanResult, NoopLogger,
    NormalRealTimeTask, OrderedMappingIdSet, OrderedMappingMap, OscDeviceId, OscFeedbackTask,
    PluginParamIndex, PluginParams, ProcessorContext, ProjectOptions, ProjectionFeedbackValue,
    QualifiedInstanceEvent, QualifiedMappingId, RawParamValue, RealTimeMappingUpdate,
    RealTimeTargetUpdate, RealearnModeContext, RealearnMonitoringFxParameterValueChangedEvent,
    RealearnParameterChangePayload, RealearnSourceContext, ReaperConfigChange, ReaperMessage,
    ReaperSourceFeedbackValue, ReaperTarget, SharedInstance, SharedUnit, SourceFeedbackEvent,
    SourceFeedbackLogger, SourceReleasedEvent, SpecificCompoundFeedbackValue, StreamDeckDeviceId,
    StreamDeckMessage, StreamDeckSourceFeedbackValue, TargetControlEvent, TargetValueChangedEvent,
    UnitContainer, UnitEvent, UnitOrchestrationEvent, UpdatedSingleMappingOnStateEvent,
    VirtualControlElement, VirtualSourceValue,
};
use derive_more::Display;
use enum_map::EnumMap;
use helgoboss_learn::{
    AbsoluteValue, ControlValue, GroupInteraction, MidiSourceValue, MinIsMaxBehavior,
    ModeControlOptions, RawMidiEvent, Target, BASE_EPSILON,
};
use std::borrow::Cow;
use std::cell::RefCell;

use crate::domain::ui_util::{
    format_control_input_with_match_result, format_incoming_midi_message, format_midi_source_value,
    format_osc_message, format_osc_packet, log_lifecycle_output, log_real_control_input,
    log_real_feedback_output, log_real_learn_input, log_target_control, log_target_output,
    log_virtual_control_input, log_virtual_feedback_output,
};
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet, NonCryptoIndexSet};
use base::{hash_util, NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use helgoboss_midi::{ControlChange14BitMessage, ParameterNumberMessage, RawShortMessage};
use playtime_api::runtime::ControlUnitId;
use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::ReaperNormalizedFxParamValue;
use rosc::{OscMessage, OscPacket, OscType};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use strum::EnumIter;
use tracing::{debug, trace};

/// This can be come pretty big when multiple track volumes are adjusted at once.
const FEEDBACK_TASK_QUEUE_SIZE: usize = 20_000;
/// This has been increased because the scenario in #913 made messages pile up in this channel.
const NORMAL_TASK_BULK_SIZE: usize = 1000;
/// This has been increased because of the insight that letting messages pile up is worse than
/// having a few cycles that take longer.
const NORMAL_RT_TASK_BULK_SIZE: usize = 1000;
/// This has been increased because of the insight that letting messages pile up is worse than
/// having a few cycles that take longer.
const FEEDBACK_TASK_BULK_SIZE: usize = 1000;
/// This has been increased because of the insight that letting messages pile up is worse than
/// having a few cycles that take longer.
const CONTROL_TASK_BULK_SIZE: usize = 1000;
// I raised this from 32 to the max channel size because of
// https://github.com/helgoboss/helgobox/issues/847 (otherwise it can easily happen with lots of
// parameter modulation that the channel runs full)
const PARAMETER_TASK_BULK_SIZE: usize = 500;

pub type SharedMainProcessors<EH> = Rc<RefCell<Vec<MainProcessor<EH>>>>;

#[derive(Debug)]
pub struct MainProcessor<EH: DomainEventHandler> {
    basics: Basics<EH>,
    collections: Collections,
    /// Contains IDs of those mappings who need to be polled as frequently as possible.
    poll_control_mappings: EnumMap<CompartmentKind, OrderedMappingIdSet>,
}

#[derive(Debug)]
struct Basics<EH: DomainEventHandler> {
    instance_id: InstanceId,
    unit_id: UnitId,
    common_lua: EnumMap<CompartmentKind, Option<mlua::Value>>,
    unit_container: &'static dyn UnitContainer,
    settings: BasicSettings,
    control_is_globally_enabled: bool,
    // TODO-medium Now that we communicate the feedback output separately, we could limit the scope
    //  of its meaning to "instance enabled etc."
    feedback_is_globally_enabled: bool,
    event_handler: EH,
    context: ProcessorContext,
    control_mode: ControlMode,
    // It's okay to not use a weak instance here because the main processor / unit lives shorter
    // than the instance. It can't lead to the instance not being disposed.
    instance: SharedInstance,
    unit: SharedUnit,
    channels: Channels,
    // Using RefCell in the processing layer is an exception. We do it here because we can't
    // safely make feedback processing mutable. I tried (see branch
    // "experiment/feedback-change-detection-mutable") but it the end it turned out to be impossible
    // because the reaper-rs control surface doesn't emit feedback-triggering events in a mutable
    // context. Rightfully so, because it's potentially reentrant!
    // TODO-low This reason is now outdated. We detected a general issue with reentrancy.
    //  https://github.com/helgoboss/reaper-rs/issues/54
    last_feedback_checksum_by_address:
        RefCell<NonCryptoHashMap<CompoundMappingSourceAddress, FeedbackChecksum>>,
    target_based_conditional_activation_processors:
        EnumMap<CompartmentKind, TargetBasedConditionalActivationProcessor>,
}

/// Used for detecting and preventing subsequent duplicate feedback.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FeedbackChecksum {
    MidiPlain(RawShortMessage),
    MidiParameterNumber(ParameterNumberMessage),
    MidiControlChange14Bit(ControlChange14BitMessage),
    // For OSC and raw MIDI
    Hashed(u64),
}

impl FeedbackChecksum {
    fn from_value(v: &FinalSourceFeedbackValue) -> Self {
        match v {
            FinalSourceFeedbackValue::Midi(v) => Self::from_midi(v),
            FinalSourceFeedbackValue::Osc(v) => Self::from_osc(v),
            FinalSourceFeedbackValue::Reaper(v) => Self::from_reaper(v),
            FinalSourceFeedbackValue::StreamDeck(v) => Self::from_stream_deck(v),
        }
    }

    fn from_midi(v: &MidiSourceValue<RawShortMessage>) -> Self {
        use MidiSourceValue::*;
        match v {
            Plain(v) => FeedbackChecksum::MidiPlain(*v),
            ParameterNumber(v) => FeedbackChecksum::MidiParameterNumber(*v),
            ControlChange14Bit(v) => FeedbackChecksum::MidiControlChange14Bit(*v),
            Raw { events, .. } => {
                let hash = hash_util::calculate_non_crypto_hash(events);
                FeedbackChecksum::Hashed(hash)
            }
            Tempo(_) | BorrowedSysEx(_) => unreachable!("never sent as feedback"),
        }
    }

    fn from_osc(v: &OscMessage) -> Self {
        let mut hasher = hash_util::create_non_crypto_hasher();
        // OscMessage doesn't implement Hash, probably because it contains floating point numbers.
        // We don't care about floating point hash/equality issues because we just want a checksum
        // for comparing current feedback with last feedback.
        v.addr.hash(&mut hasher);
        for arg in &v.args {
            hash_osc_arg(arg, &mut hasher);
        }
        FeedbackChecksum::Hashed(hasher.finish())
    }

    fn from_stream_deck(v: &StreamDeckSourceFeedbackValue) -> Self {
        let mut hasher = hash_util::create_non_crypto_hasher();
        // Doesn't implement Hash, probably because it contains floating point numbers.
        // We don't care about floating point hash/equality issues because we just want a checksum
        // for comparing current feedback with last feedback.
        v.hash(&mut hasher);
        FeedbackChecksum::Hashed(hasher.finish())
    }

    fn from_reaper(v: &ReaperSourceFeedbackValue) -> Self {
        match v {
            ReaperSourceFeedbackValue::Speech(s) => {
                let hash = hash_util::calculate_non_crypto_hash_one_shot(s.text.as_bytes());
                FeedbackChecksum::Hashed(hash)
            }
        }
    }
}

fn hash_osc_arg<H: Hasher>(arg: &OscType, hasher: &mut H) {
    use OscType::*;
    match arg {
        Int(v) => {
            (0, v).hash(hasher);
        }
        Float(v) => {
            (1, v.to_ne_bytes()).hash(hasher);
        }
        String(v) => {
            (2, v).hash(hasher);
        }
        Blob(v) => {
            (3, v).hash(hasher);
        }
        Time(v) => {
            (4, v).hash(hasher);
        }
        Long(v) => {
            (5, v).hash(hasher);
        }
        Double(v) => {
            (6, v.to_ne_bytes()).hash(hasher);
        }
        Char(v) => {
            (7, v).hash(hasher);
        }
        Color(v) => {
            (8, (v.red, v.green, v.red, v.alpha)).hash(hasher);
        }
        Midi(v) => {
            (9, (v.port, v.status, v.data1, v.data2)).hash(hasher);
        }
        Bool(v) => {
            (10, v).hash(hasher);
        }
        Array(v) => {
            11.hash(hasher);
            for a in &v.content {
                hash_osc_arg(a, hasher);
            }
        }
        Nil => {
            12.hash(hasher);
        }
        Inf => {
            13.hash(hasher);
        }
    }
}

#[derive(Debug)]
struct Collections {
    /// Contains mappings without virtual targets.
    mappings: EnumMap<CompartmentKind, OrderedMappingMap<MainMapping>>,
    /// Contains mappings with virtual targets.
    mappings_with_virtual_targets: OrderedMappingMap<MainMapping>,
    /// Contains IDs of those mappings which should be refreshed as soon as a target is touched.
    /// At the moment only "Last touched" targets.
    target_touch_dependent_mappings: EnumMap<CompartmentKind, OrderedMappingIdSet>,
    /// Contains IDs of those mappings whose feedback might change depending on the current beat.
    beat_dependent_feedback_mappings: EnumMap<CompartmentKind, OrderedMappingIdSet>,
    /// Contains IDs of those mappings whose feedback might change depending on the current milli.
    /// TODO-low The mappings in there are polled regularly (even if main timeline is not playing).
    ///  could be optimized. However, this is what makes the seek target work currently when
    ///  changing cursor position while stopped.
    milli_dependent_feedback_mappings: EnumMap<CompartmentKind, OrderedMappingIdSet>,
    parameters: PluginParams,
    previous_target_values: EnumMap<CompartmentKind, NonCryptoHashMap<MappingId, AbsoluteValue>>,
}

#[derive(Debug)]
struct Channels {
    self_feedback_sender: SenderToNormalThread<FeedbackMainTask>,
    self_normal_sender: SenderToNormalThread<NormalMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    normal_real_time_to_main_thread_task_receiver:
        crossbeam_channel::Receiver<NormalRealTimeToMainThreadTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    unit_event_receiver: crossbeam_channel::Receiver<UnitEvent>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: SenderToRealTimeThread<NormalRealTimeTask>,
    feedback_real_time_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
    feedback_audio_hook_task_sender: SenderToRealTimeThread<FeedbackAudioHookTask>,
    osc_feedback_task_sender: SenderToNormalThread<OscFeedbackTask>,
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    unit_orchestration_event_sender: SenderToNormalThread<UnitOrchestrationEvent>,
    integration_test_feedback_sender: Option<SenderToNormalThread<FinalSourceFeedbackValue>>,
}

impl<EH: DomainEventHandler> MainProcessor<EH> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: InstanceId,
        unit_id: UnitId,
        self_normal_sender: SenderToNormalThread<NormalMainTask>,
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        normal_real_time_to_main_thread_task_receiver: crossbeam_channel::Receiver<
            NormalRealTimeToMainThreadTask,
        >,
        parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        instance_feedback_event_receiver: crossbeam_channel::Receiver<UnitEvent>,
        normal_real_time_task_sender: SenderToRealTimeThread<NormalRealTimeTask>,
        feedback_real_time_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
        feedback_audio_hook_task_sender: SenderToRealTimeThread<FeedbackAudioHookTask>,
        additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
        instance_orchestration_event_sender: SenderToNormalThread<UnitOrchestrationEvent>,
        osc_feedback_task_sender: SenderToNormalThread<OscFeedbackTask>,
        event_handler: EH,
        context: ProcessorContext,
        instance: SharedInstance,
        unit: SharedUnit,
        instance_container: &'static dyn UnitContainer,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) =
            SenderToNormalThread::new_bounded_channel(
                "feedback main tasks",
                FEEDBACK_TASK_QUEUE_SIZE,
            );
        MainProcessor {
            basics: Basics {
                instance_id,
                unit_id,
                common_lua: Default::default(),
                settings: Default::default(),
                control_is_globally_enabled: false,
                feedback_is_globally_enabled: false,
                event_handler,
                context,
                control_mode: ControlMode::Controlling,
                instance,
                unit,
                unit_container: instance_container,
                channels: Channels {
                    self_feedback_sender,
                    self_normal_sender,
                    normal_task_receiver,
                    normal_real_time_to_main_thread_task_receiver,
                    feedback_task_receiver,
                    parameter_task_receiver,
                    unit_event_receiver: instance_feedback_event_receiver,
                    control_task_receiver,
                    normal_real_time_task_sender,
                    feedback_real_time_task_sender,
                    feedback_audio_hook_task_sender,
                    osc_feedback_task_sender,
                    additional_feedback_event_sender,
                    unit_orchestration_event_sender: instance_orchestration_event_sender,
                    integration_test_feedback_sender: None,
                },
                last_feedback_checksum_by_address: Default::default(),
                target_based_conditional_activation_processors: Default::default(),
            },
            collections: Collections {
                mappings: Default::default(),
                mappings_with_virtual_targets: Default::default(),
                target_touch_dependent_mappings: Default::default(),
                beat_dependent_feedback_mappings: Default::default(),
                milli_dependent_feedback_mappings: Default::default(),
                parameters: Default::default(),
                previous_target_values: Default::default(),
            },
            poll_control_mappings: Default::default(),
        }
    }

    pub fn unit_id(&self) -> UnitId {
        self.basics.unit_id
    }

    /// This is the chance to take over a source from another instance (send our feedback).
    ///
    /// This is a very important principle when using multiple instances. It allows feedback to
    /// not be accidentally cleared while still guaranteeing that feedback for non-used control
    /// elements are cleared eventually - independently from the order of instance processing.
    pub fn maybe_takeover_source(&self, released_event: &SourceReleasedEvent) -> bool {
        if released_event.feedback_output != self.basics.settings.feedback_output {
            // Difference feedback device. No source takeover of course.
            return false;
        }
        if let Some(mapping_with_source) = self.all_mappings().find(|m| {
            m.feedback_is_effectively_on()
                && m.source().has_same_feedback_address_as_value(
                    &released_event.feedback_value,
                    self.basics.source_context(m.compartment()),
                )
        }) {
            if let Some(followed_mapping) = self.follow_maybe_virtual_mapping(mapping_with_source) {
                if self.basics.unit_feedback_is_effectively_enabled() {
                    debug!("Taking over source {:?}...", mapping_with_source.source());
                    // TODO-low Shouldn't we update the single mapping-on state here?
                    let feedback = followed_mapping.feedback(
                        true,
                        self.basics.control_context(followed_mapping.compartment()),
                    );
                    self.send_feedback(FeedbackReason::TakeOverSource, feedback);
                    true
                } else {
                    debug!(
                        "No source takeover of {:?} because feedback effectively disabled",
                        mapping_with_source.source()
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
        feedback_output: Option<FeedbackOutput>,
        feedback_value: FinalSourceFeedbackValue,
    ) {
        debug!("Finally switching off source with {:?}...", feedback_value);
        self.basics.send_direct_source_feedback(
            feedback_output,
            FeedbackReason::FinallySwitchOffSource,
            FeedbackCause::Normal,
            feedback_value,
        );
    }

    /// Processes control tasks coming from the real-time processor.
    ///
    /// This should *not* be called by the control surface when it's globally learning targets
    /// because we want to pause controlling in that case! Otherwise we could control targets and
    /// they would be learned although not touched via mouse, that's not good.
    pub fn run_control(&mut self, timestamp: ControlEventTimestamp) {
        let control_is_effectively_enabled = self.basics.unit_control_is_effectively_enabled();
        // Collect control tasks (we do that in any case to not let get channels full).
        let mut count = 0;
        while let Ok(task) = self.basics.channels.control_task_receiver.try_recv() {
            // It's possible that control is disabled because another instance cancels us. In that case
            // the RealTimeProcessor won't know about it and keeps sending MIDI. Stop it here!
            if control_is_effectively_enabled {
                self.process_control_task(task);
            }
            count += 1;
            if count == CONTROL_TASK_BULK_SIZE {
                break;
            }
        }
        self.poll_control(timestamp);
    }

    fn process_control_task(&mut self, task: ControlMainTask) {
        use ControlMainTask::*;
        match task {
            ControlFromRealTime {
                compartment,
                mapping_id,
                event,
                options,
            } => {
                let _ = self.control_from_real_time(compartment, mapping_id, event, options);
            }
            LogVirtualControlInput {
                event: value,
                match_outcome: match_result,
            } => {
                log_virtual_control_input(
                    self.unit_id(),
                    format_control_input_with_match_result(value, match_result),
                );
            }
            LogRealControlInput {
                event,
                match_outcome: match_result,
            } => {
                let timestamp = event.timestamp();
                log_real_control_input(
                    self.unit_id(),
                    format_control_input_with_match_result(
                        ControlEvent::new(
                            format_midi_source_value(&event.into_payload()),
                            timestamp,
                        ),
                        match_result,
                    ),
                );
            }
            LogRealLearnInput { event } => {
                let timestamp = event.timestamp();
                log_real_learn_input(
                    self.unit_id(),
                    ControlEvent::new(
                        format_incoming_midi_message(event.into_payload()),
                        timestamp,
                    ),
                );
            }
            LogTargetOutput { event } => {
                log_target_output(self.unit_id(), format_as_pretty_hex(event.bytes()));
            }
            LogTargetControl { mapping_id, entry } => {
                let logger = self
                    .basics
                    .target_control_logger(ControlLogContext::RealTime, mapping_id);
                logger(entry);
            }
        }
    }

    fn poll_control(&mut self, timestamp: ControlEventTimestamp) {
        for compartment in CompartmentKind::enum_iter() {
            for id in self.poll_control_mappings[compartment].iter() {
                let (is_source_poll, control_result, group_interaction) = if let Some(m) =
                    self.collections.mappings[compartment].get_mut(id)
                {
                    let control_context = self.basics.control_context(compartment);
                    let processor_context = ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    );
                    let mode_poll_result = if m.mode().wants_to_be_polled() {
                        m.poll_mode(
                            control_context,
                            processor_context,
                            timestamp,
                            self.basics.target_control_logger(
                                ControlLogContext::Polling,
                                m.qualified_id(),
                            ),
                        )
                    } else {
                        Default::default()
                    };
                    let (is_source_poll, mut final_poll_result) = if mode_poll_result
                        .at_least_one_target_was_reached
                    {
                        // Mode was polled successfully. This one has precedence.
                        // We poll even if control is effectively off because it might have been
                        // on before and user might have pressed a button which started some
                        // timer - and we still want that timer to fire. This is practical e.g.
                        // when having a single-press button with a modifier. It's not uncommon
                        // to shortly press the modifier, press the single-press button and
                        // release the modifier. If we wouldn't poll anymore in that case, the
                        // single press would be discarded - or worse, fired when the mapping
                        // is enabled again.
                        (false, mode_poll_result)
                    } else if m.source().wants_to_be_polled() && m.control_is_effectively_on() {
                        // Mode was either not polled at all or without result, poll source.
                        let res = if let Some(source_control_value) = m.poll_source() {
                            let control_event = ControlEvent::new(source_control_value, timestamp);
                            control_mapping_stage_one(
                                &self.basics,
                                &self.collections.parameters,
                                m,
                                control_event,
                                ControlOptions::default(),
                            )
                        } else {
                            Default::default()
                        };
                        (true, res)
                    } else {
                        // Mode was either not polled at all or without result, source doesn't
                        // want to be polled.
                        (false, mode_poll_result)
                    };
                    control_mapping_stage_two(
                        &self.basics,
                        &mut final_poll_result,
                        m,
                        ManualFeedbackProcessing::On {
                            mappings_with_virtual_targets: &self
                                .collections
                                .mappings_with_virtual_targets,
                        },
                    );
                    (is_source_poll, final_poll_result, m.group_interaction())
                } else {
                    continue;
                };

                // When this is a mode poll, we only do target-value based group interaction after
                // polling (makes sense because control-value based one has been done at control
                // time already).
                let needs_group_interaction = control_result.at_least_one_target_was_reached
                    && (is_source_poll || group_interaction.is_target_based());
                control_mapping_stage_three(
                    &self.basics,
                    &mut self.collections,
                    compartment,
                    control_result,
                    if needs_group_interaction {
                        GroupInteractionProcessing::On(GroupInteractionInput {
                            mapping_id: *id,
                            // Control value is not important because we only do target-value
                            // based group interaction.
                            control_event: ControlEvent::new(
                                ControlValue::AbsoluteContinuous(Default::default()),
                                timestamp,
                            ),
                            group_interaction,
                        })
                    } else {
                        GroupInteractionProcessing::Off
                    },
                );
            }
        }
    }

    /// Processes incoming control messages from the real-time processor.
    fn control_from_real_time(
        &mut self,
        compartment: CompartmentKind,
        mapping_id: MappingId,
        control_event: ControlEvent<ControlValue>,
        options: ControlOptions,
    ) -> Result<(), &'static str> {
        // Resolving mappings with virtual targets is not necessary anymore. It has
        // been done in the real-time processor already.
        let (control_result, group_interaction) = {
            let m = self.collections.mappings[compartment]
                .get_mut(&mapping_id)
                .ok_or("mapping not found")?;
            // Most of the time, the main processor won't even receive a MIDI-triggered control
            // instruction from the real-time processor for a mapping for which control is disabled,
            // because the real-time processor doesn't process disabled mappings. But if control is
            // (temporarily) disabled because a target condition is (temporarily) not met (e.g.
            // "track must be selected") and the real-time processor doesn't yet know about it,
            // there might be a short amount of time where we still receive control statements. We
            // filter them here.
            if !m.control_is_effectively_on() {
                return Ok(());
            }
            let control_result = control_mapping_stage_one_and_two(
                &self.basics,
                &self.collections.parameters,
                m,
                control_event,
                options,
                ManualFeedbackProcessing::On {
                    mappings_with_virtual_targets: &self.collections.mappings_with_virtual_targets,
                },
            );
            (control_result, m.group_interaction())
        };
        control_mapping_stage_three(
            &self.basics,
            &mut self.collections,
            compartment,
            control_result,
            GroupInteractionProcessing::On(GroupInteractionInput {
                mapping_id,
                control_event,
                group_interaction,
            }),
        );
        Ok(())
    }

    /// This should be regularly called by the control surface, even during global target learning.
    pub fn run_essential(&mut self, timestamp: ControlEventTimestamp) {
        self.process_normal_tasks_from_real_time_processor();
        self.process_normal_tasks_from_session(timestamp);
        self.process_parameter_tasks();
        self.process_feedback_tasks();
        self.process_unit_events();
        self.poll_for_feedback();
    }

    /// This goes through all mappings that returned "high" feedback resolution - which they do if
    /// there are no appropriate change events to listen to and therefore need feedback polling.
    #[allow(clippy::float_cmp)]
    fn poll_for_feedback(&mut self) {
        for compartment in CompartmentKind::enum_iter() {
            for mapping_id in self.collections.milli_dependent_feedback_mappings[compartment].iter()
            {
                if let Some(m) = self.collections.mappings[compartment].get(mapping_id) {
                    let previous_target_values = &mut self.collections.previous_target_values;
                    let control_context = self.basics.control_context(compartment);
                    self.basics
                        .process_feedback_related_reaper_event_for_mapping(
                            m,
                            &self.collections.mappings_with_virtual_targets,
                            &mut |m, t| {
                                if m.mode().feedback_props_in_use().is_empty() {
                                    // No feedback props are used, which means we have pure
                                    // numeric feedback (no textual feedback, no prop-based feedback
                                    // style settings).
                                    // Numeric feedback is always in percentages, so we can
                                    // safely block feedback already here if we encounter
                                    // duplicate target values. So check for duplicate feedback!
                                    // TODO-high-discrete Maybe not true anymore with discrete
                                    //  targets.
                                    let (affected, new_value) = if let Some(value) =
                                        t.current_value(control_context)
                                    {
                                        // Check if changed
                                        match previous_target_values[compartment].entry(*mapping_id)
                                        {
                                            Entry::Occupied(mut e) => {
                                                // We really want to resend if there's the slightest
                                                // difference. It's okay to have direct comparison
                                                // because we know the source of these two values is
                                                // the same.
                                                if e.get().to_unit_value().get()
                                                    == value.to_unit_value().get()
                                                {
                                                    // Value hasn't changed.
                                                    (false, None)
                                                } else {
                                                    // Value has changed.
                                                    e.insert(value);
                                                    (true, Some(value))
                                                }
                                            }
                                            Entry::Vacant(e) => {
                                                // No feedback sent yet for that milli-dependent mapping.
                                                e.insert(value);
                                                (true, Some(value))
                                            }
                                        }
                                    } else {
                                        // Couldn't determine feedback value.
                                        (false, None)
                                    };
                                    if affected {
                                        m.update_last_non_performance_target_value_if_appropriate(
                                            new_value,
                                        );
                                    }
                                    (affected, new_value)
                                } else {
                                    // We use feedback props. That either means we have numeric
                                    // feedback with some prop-based feedback style or we have
                                    // text feedback.
                                    //
                                    // Props can change even if the main target value doesn't
                                    // change!
                                    //
                                    // Also, text feedback is not necessarily based on percentages.
                                    // This means we can have the situation that in terms of
                                    // percentages (usually relevant for control direction), the
                                    // current value might be below 0% or above 100%, which would
                                    // let the percentage (unit value) stay the same. But the
                                    // text feedback might go beyond that interval, so we should
                                    // always update it! Example: Seek target with "Use project"
                                    // enabled.

                                    // We are now required to return the current target value.
                                    let new_value = t.current_value(control_context);
                                    (true, new_value)
                                }
                            },
                        );
                }
            }
        }
    }

    fn process_unit_events(&mut self) {
        let mut changes_conditions = false;
        for event in self
            .basics
            .channels
            .unit_event_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
        {
            // Propagate to other instances if necessary
            if event.is_interesting_for_other_units() {
                let global_event = AdditionalFeedbackEvent::Unit {
                    unit_id: self.basics.unit_id,
                    unit_event: event.clone(),
                };
                self.basics
                    .channels
                    .additional_feedback_event_sender
                    .send_complaining(global_event);
            }
            // Process feedback
            self.process_feedback_related_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::Unit(&event),
                    self.basics.control_context(mapping.compartment()),
                )
            });
            // Check if this event changes conditions
            if ReaperTarget::changes_conditions(CompoundChangeEvent::Unit(&event)) {
                changes_conditions = true;
            }
        }
        if changes_conditions {
            self.basics.notify_conditions_changed();
        }
    }

    /// Processes the given Playtime matrix events if they are relevant to this unit.
    #[cfg(feature = "playtime")]
    pub fn process_polled_clip_matrix_events(
        &self,
        instance_id: InstanceId,
        events: &[playtime_clip_engine::base::ClipMatrixEvent],
    ) {
        if instance_id != self.basics.instance_id {
            return;
        }
        for event in events {
            self.process_clip_matrix_event_for_feedback(event);
        }
    }

    /// Processes the given Playtime matrix event if it's relevant to this instance.
    #[cfg(feature = "playtime")]
    pub fn process_non_polled_clip_matrix_event(
        &self,
        event: &crate::domain::QualifiedClipMatrixEvent,
    ) {
        if event.instance_id != self.basics.instance_id {
            return;
        }
        self.process_clip_matrix_event_for_feedback(&event.event)
    }

    pub fn process_instance_event_for_feedback(&self, event: &QualifiedInstanceEvent) {
        if event.instance_id != self.basics.instance_id {
            return;
        }
        self.process_feedback_related_event(|mapping, target| {
            mapping.process_change_event(
                target,
                CompoundChangeEvent::Instance(&event.event),
                self.basics.control_context(mapping.compartment()),
            )
        });
    }

    #[cfg(feature = "playtime")]
    fn process_clip_matrix_event_for_feedback(
        &self,
        event: &playtime_clip_engine::base::ClipMatrixEvent,
    ) {
        let is_position_change = matches!(
            event,
            playtime_clip_engine::base::ClipMatrixEvent::SlotChanged(
                playtime_clip_engine::rt::QualifiedSlotChangeEvent {
                    event: playtime_clip_engine::rt::SlotChangeEvent::Continuous { .. },
                    ..
                }
            )
        );
        if is_position_change {
            // Position changed. This happens very frequently when a clip is playing.
            // Mappings with slot seek targets are in the beat-dependent feedback
            // mapping set, not in the milli-dependent one (because we don't want to
            // query their feedback value more than once in one main loop cycle).
            // So we don't want to iterate over all mappings but just the beat-dependent
            // ones.
            for compartment in CompartmentKind::enum_iter() {
                for mapping_id in
                    self.collections.beat_dependent_feedback_mappings[compartment].iter()
                {
                    if let Some(m) = self.collections.mappings[compartment].get(mapping_id) {
                        self.process_feedback_related_reaper_event_for_mapping(
                            m,
                            &mut |m, target| {
                                m.process_change_event(
                                    target,
                                    CompoundChangeEvent::ClipMatrix(event),
                                    self.basics.control_context(m.compartment()),
                                )
                            },
                        );
                    }
                }
            }
        } else {
            if ReaperTarget::changes_conditions(CompoundChangeEvent::ClipMatrix(event)) {
                self.basics.notify_conditions_changed();
            }
            // Other property of clip changed.
            self.process_feedback_related_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::ClipMatrix(event),
                    self.basics.control_context(mapping.compartment()),
                )
            });
        }
    }

    fn process_feedback_tasks(&mut self) {
        let mut count = 0;
        while let Ok(task) = self.basics.channels.feedback_task_receiver.try_recv() {
            use FeedbackMainTask::*;
            match task {
                MappingTargetValueChanged {
                    lead_mapping_id,
                    target_value,
                } => {
                    self.process_conditional_activation_target_value_change(
                        lead_mapping_id,
                        true,
                        Some(target_value),
                    );
                }
            }
            count += 1;
            if count == FEEDBACK_TASK_BULK_SIZE {
                break;
            }
        }
    }

    /// Handles the change of a target value of a lead mapping in terms of target-based conditional
    /// activation.
    ///
    /// Pass the target value at hand, usually the result of querying the mapping to return the
    /// aggregated target value. This method takes care of correctly handling the situation that
    /// we have a target value but the target is inactive.
    fn process_conditional_activation_target_value_change(
        &mut self,
        id: QualifiedMappingId,
        target_is_active: bool,
        target_value: Option<AbsoluteValue>,
    ) {
        // Calculate final target value
        let target_value = match determine_final_target_value_for_conditional_activation(
            target_is_active,
            target_value,
        ) {
            Err(_) => return,
            Ok(v) => v,
        };
        // Process it
        let compartment = id.compartment;
        let activation_effects: Vec<MappingActivationEffect> = self
            .basics
            .target_based_conditional_activation_processors[compartment]
            .get_follow_mappings(id.id)
            .filter_map(|follow_mapping_id| {
                let follow_mapping = get_normal_or_virtual_target_mapping_mut(
                    &mut self.collections.mappings,
                    &mut self.collections.mappings_with_virtual_targets,
                    compartment,
                    follow_mapping_id,
                )?;
                follow_mapping.check_activation_effect_of_target_value_update(id.id, target_value)
            })
            .collect();
        self.process_activation_effects(compartment, activation_effects, false);
    }

    fn process_change_of_last_touched_target(&mut self) {
        // The last touched target has changed! We re-resolve all "Last touched" targets.
        for compartment in CompartmentKind::enum_iter() {
            for mapping_id in self.collections.target_touch_dependent_mappings[compartment].iter() {
                // Virtual targets are not candidates for "Last touched" so we don't
                // need to consider them here.
                let fb = if let Some(m) = self.collections.mappings[compartment].get_mut(mapping_id)
                {
                    // We don't need to track activation updates because this target
                    // is always on. Switching off is not necessary since the last
                    // touched target can never be "unset".
                    let control_context = self.basics.control_context(m.compartment());
                    let _ = m.refresh_target(
                        ExtendedProcessorContext::new(
                            &self.basics.context,
                            &self.collections.parameters,
                            control_context,
                        ),
                        control_context,
                    );
                    if m.has_reaper_target() && m.has_resolved_successfully() {
                        if m.feedback_is_effectively_on() {
                            // TODO-medium Is this executed too frequently and maybe
                            // even sends redundant feedback!?
                            m.feedback(true, control_context)
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

    fn process_parameter_tasks(&mut self) {
        // In theory, this could be optimized by reducing the number of changes. E.g. if we
        // find that we have multiple changes of one parameter, we could choose to just process
        // the last one. I wanted to do that as part of
        // https://github.com/helgoboss/helgobox/issues/847 but chose to not do it ... because
        // this optimization could potentially change control behavior if that parameter is used
        // for control. Imagine a toggle mode ... then the number of invocations is relevant.
        let mut count = 0;
        while let Ok(task) = self.basics.channels.parameter_task_receiver.try_recv() {
            use ParameterMainTask::*;
            match task {
                UpdateAllParams(params) => {
                    self.update_all_params(params);
                }
                UpdateSingleParamValue {
                    index,
                    value,
                    timestamp,
                } => self.update_single_param_value(index, value, timestamp),
            }
            count += 1;
            if count == PARAMETER_TASK_BULK_SIZE {
                break;
            }
        }
    }

    // https://github.com/rust-lang/rust-clippy/issues/6066
    #[allow(clippy::needless_collect)]
    fn update_single_param_value(
        &mut self,
        index: PluginParamIndex,
        value: RawParamValue,
        timestamp: ControlEventTimestamp,
    ) {
        debug!("Updating parameter {} to {}...", index, value);
        // Work around REAPER's inability to notify about parameter changes in
        // monitoring FX by simulating the notification ourselves.
        // Then parameter learning and feedback works at least for
        // ReaLearn monitoring FX instances, which is especially
        // useful for conditional activation.
        if self.basics.context.is_on_monitoring_fx_chain() {
            let parameter = self
                .basics
                .context
                .containing_fx()
                .parameter_by_index(index.get());
            self.basics
                .channels
                .additional_feedback_event_sender
                .send_complaining(
                    AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(
                        RealearnMonitoringFxParameterValueChangedEvent {
                            parameter,
                            new_value: ReaperNormalizedFxParamValue::new(value as _),
                        },
                    ),
                );
        }
        // Update own value (important to do first)
        let param = self.collections.parameters.at_mut(index);
        let previous_value = param.raw_value();
        param.set_raw_value(value);
        // When our own parameters change, it's THE example of a condition change.
        self.basics.notify_conditions_changed();
        // Let compartment parameter targets know
        self.process_compartment_parameter_feedback(index);
        // Notify domain event handler
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::UpdatedSingleParameterValue { index, value });
        // Determine and process activation effects
        let compartment = CompartmentKind::by_plugin_param_index(index);
        let activation_effects: Vec<MappingActivationEffect> = self
            .all_mappings_in_compartment(compartment)
            .filter_map(|m| {
                m.check_activation_effect_of_param_update(
                    &self.collections.parameters,
                    index,
                    previous_value,
                )
            })
            .collect();
        self.process_activation_effects(compartment, activation_effects, true);
        // Control ("Helgobox parameter source")
        let control_payload = RealearnParameterChangePayload {
            compartment,
            parameter_index: compartment.to_compartment_param_index(index),
            value,
        };
        let control_msg = ReaperMessage::RealearnParameterChange(control_payload);
        if self.basics.settings.real_input_logging_enabled {
            self.log_incoming_message(&control_msg);
        }
        let control_event = ControlEvent::new(MainSourceMessage::Reaper(&control_msg), timestamp);
        self.process_incoming_message_internal(control_event);
    }

    /// This is called after determining mapping and/or target activation changes.
    fn process_activation_effects(
        &mut self,
        compartment: CompartmentKind,
        activation_effects: Vec<MappingActivationEffect>,
        refresh_targets: bool,
    ) {
        if activation_effects.is_empty() && !refresh_targets {
            return;
        }
        // Mapping activation is supported for both compartments and target activation
        // might change also in non-virtual controller mappings due to dynamic targets.
        let mut changed_mappings = NonCryptoIndexSet::default();
        let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
        // In order to avoid a mutable borrow of mappings and an immutable borrow of
        // parameters at the same time, we need to separate into READ activation
        // effects and WRITE activation updates.
        // 2. Mapping activation: Write
        let mapping_updates: Vec<RealTimeMappingUpdate> = activation_effects
            .into_iter()
            .filter_map(|eff| {
                changed_mappings.insert(eff.id);
                let m = get_normal_or_virtual_target_mapping_mut(
                    &mut self.collections.mappings,
                    &mut self.collections.mappings_with_virtual_targets,
                    compartment,
                    eff.id,
                )?;
                m.update_activation_from_effect(eff)
            })
            .collect();
        // 3. Mappings with real targets: Refresh targets and determine unused sources
        let mut target_updates: Vec<RealTimeTargetUpdate> = vec![];
        for m in self.collections.mappings[compartment].values_mut() {
            if refresh_targets && m.target_can_be_affected_by_parameters() {
                let control_context = self.basics.control_context(m.compartment());
                let context = ExtendedProcessorContext::new(
                    &self.basics.context,
                    &self.collections.parameters,
                    control_context,
                );
                if let Some(target_update) = m.refresh_target(context, control_context) {
                    target_updates.push(target_update);
                    changed_mappings.insert(m.id());
                }
            }
            if m.feedback_is_effectively_on() {
                // Mark source as used
                if let Some(addr) = m
                    .source()
                    .extract_feedback_address(self.basics.source_context(m.compartment()))
                {
                    unused_sources.remove(&addr);
                }
            }
        }
        // 4. Mappings with virtual targets: Determine unused sources
        if compartment == CompartmentKind::Controller {
            for m in self.collections.mappings_with_virtual_targets.values() {
                if !m.feedback_is_effectively_on() {
                    continue;
                }
                // A mapping with virtual target is only considered as active if there's a
                // corresponding active main mapping.
                // (https://github.com/helgoboss/helgobox/issues/563)
                let has_active_main_mapping =
                    find_active_main_mapping_connected_to_virtual_control_element(
                        &self.collections.mappings[CompartmentKind::Main],
                        m.virtual_target_control_element().unwrap(),
                    )
                    .is_some();
                if !has_active_main_mapping {
                    continue;
                }
                // Mark source as used
                if let Some(addr) = m
                    .source()
                    .extract_feedback_address(self.basics.source_context(m.compartment()))
                {
                    unused_sources.remove(&addr);
                }
            }
        }
        self.process_mapping_updates_due_to_activation_changes(
            compartment,
            mapping_updates,
            target_updates,
            unused_sources,
            changed_mappings.into_iter(),
        );
    }

    fn update_all_params(&mut self, params: PluginParams) {
        debug!("Updating all parameters...");
        self.collections.parameters = params.clone();
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::UpdatedAllParameters(params));
        for compartment in CompartmentKind::enum_iter() {
            let mut mapping_updates: Vec<RealTimeMappingUpdate> = vec![];
            let mut target_updates: Vec<RealTimeTargetUpdate> = vec![];
            let mut changed_mappings = vec![];
            let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
            for m in all_mappings_in_compartment_mut(
                &mut self.collections.mappings,
                &mut self.collections.mappings_with_virtual_targets,
                compartment,
            ) {
                if m.activation_can_be_affected_by_parameters() {
                    if let Some(update) =
                        m.update_activation_from_params(&self.collections.parameters)
                    {
                        mapping_updates.push(update);
                        changed_mappings.push(m.id())
                    }
                }
                if m.target_can_be_affected_by_parameters() {
                    let control_context = self.basics.control_context(m.compartment());
                    let context = ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    );
                    if let Some(target_update) = m.refresh_target(context, control_context) {
                        target_updates.push(target_update);
                        changed_mappings.push(m.id())
                    }
                }
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m
                        .source()
                        .extract_feedback_address(self.basics.source_context(compartment))
                    {
                        unused_sources.remove(&addr);
                    }
                }
            }
            self.process_mapping_updates_due_to_activation_changes(
                compartment,
                mapping_updates,
                target_updates,
                unused_sources,
                changed_mappings.into_iter(),
            );
        }
    }

    fn process_normal_tasks_from_session(&mut self, timestamp: ControlEventTimestamp) {
        let mut count = 0;
        let mut conditions_changed = false;
        while let Ok(task) = self.basics.channels.normal_task_receiver.try_recv() {
            use NormalMainTask::*;
            match task {
                UpdateSettings(settings) => {
                    self.update_settings(settings);
                }
                UpdateCompartmentSettings(compartment, settings) => {
                    self.update_compartment_settings(compartment, settings);
                }
                UpdateAllMappings(compartment, mappings) => {
                    self.update_all_mappings(compartment, mappings);
                }
                NotifyRealearnUnitStarted => {
                    debug!("NotifyRealearnUnitStarted received");
                    let evt = ControlEvent::new(&ReaperMessage::RealearnUnitStarted, timestamp);
                    self.process_reaper_message(evt);
                }
                NotifyRealearnCompartmentLoaded(kind) => {
                    debug!("NotifyRealearnCompartmentLoaded received");
                    let message = ReaperMessage::RealearnCompartmentLoaded(kind);
                    let evt = ControlEvent::new(&message, timestamp);
                    self.process_reaper_message(evt);
                }
                HitTarget { id, value } => {
                    self.hit_target(id, value);
                }
                NotifyConditionsChanged => {
                    conditions_changed = true;
                }
                UpdateSingleMapping(mapping) => {
                    self.update_single_mapping(mapping);
                }
                UpdatePersistentMappingProcessingState { id, state } => {
                    self.update_persistent_mapping_processing_state(id, state);
                }
                SendAllFeedback => {
                    self.send_all_feedback();
                }
                LogDebugInfo => {
                    self.log_debug_info();
                }
                LogMapping(compartment, mapping_id) => {
                    self.log_mapping(compartment, mapping_id);
                }
                StartLearnSource {
                    allow_virtual_sources,
                    osc_arg_index_hint,
                } => {
                    debug!("Start learning source");
                    self.basics
                        .channels
                        .normal_real_time_task_sender
                        .send_complaining(NormalRealTimeTask::StartLearnSource {
                            allow_virtual_sources,
                        });
                    self.basics.control_mode = ControlMode::LearningSource {
                        allow_virtual_sources,
                        osc_arg_index_hint,
                    };
                }
                DisableControl => {
                    debug!("Disable control");
                    self.basics
                        .channels
                        .normal_real_time_task_sender
                        .send_complaining(NormalRealTimeTask::DisableControl);
                    self.basics.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    debug!("Return to control mode");
                    self.basics
                        .channels
                        .normal_real_time_task_sender
                        .send_complaining(NormalRealTimeTask::ReturnToControlMode);
                    self.basics.control_mode = ControlMode::Controlling;
                }
                UseIntegrationTestFeedbackSender(sender) => {
                    self.basics.channels.integration_test_feedback_sender = Some(sender);
                }
                PotentiallyEnableOrDisableControlOrFeedback => {
                    self.potentially_enable_or_disable_control_or_feedback(
                        self.any_main_mapping_is_effectively_on(),
                    );
                }
            }
            count += 1;
            if count == NORMAL_TASK_BULK_SIZE {
                break;
            }
        }
        if conditions_changed {
            self.process_changed_conditions();
        }
    }

    fn potentially_enable_or_disable_control_or_feedback(
        &mut self,
        any_main_mapping_is_effectively_on: bool,
    ) {
        let project_options = get_project_options();
        self.potentially_enable_or_disable_control(
            any_main_mapping_is_effectively_on,
            project_options,
        );
        self.potentially_enable_or_disable_feedback(
            any_main_mapping_is_effectively_on,
            project_options,
        );
        let event = GlobalControlAndFeedbackState {
            control_active: self.basics.control_is_globally_enabled,
            feedback_active: self.basics.feedback_is_globally_enabled,
        };
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::GlobalControlAndFeedbackStateChanged(event));
    }

    fn potentially_enable_or_disable_control(
        &mut self,
        any_main_mapping_is_effectively_on: bool,
        project_options: ProjectOptions,
    ) {
        self.basics.potentially_enable_or_disable_control_internal(
            any_main_mapping_is_effectively_on,
            project_options,
        );
    }

    fn potentially_enable_or_disable_feedback(
        &mut self,
        any_main_mapping_is_effectively_on: bool,
        project_options: ProjectOptions,
    ) {
        let new_feedback_is_enabled = self.basics.potentially_enable_or_disable_feedback_internal(
            any_main_mapping_is_effectively_on,
            project_options,
        );
        if let Some(new_feedback_is_enabled) = new_feedback_is_enabled {
            if new_feedback_is_enabled {
                for compartment in CompartmentKind::enum_iter() {
                    self.handle_feedback_after_having_updated_all_mappings(
                        compartment,
                        HashMap::default(),
                    );
                }
            } else {
                // Clear it completely. Other instances that might take over maybe don't use
                // all control elements and we don't want to leave traces.
                self.clear_all_feedback_allowing_source_takeover();
            };
        }
    }

    /// This should be called on events such as track list change, FX focus etc.
    ///
    /// It will trigger a refresh of all targets (re-resolve) or even a preset change (if
    /// auto-load is enabled).
    ///
    /// Shouldn't be called directly when the REAPER change event occurs but in the next main loop
    /// cycle. That's especially important for auto-load because REAPER first needs to digest info
    /// such as "Is the window open?" and "What FX is the focused FX?".
    fn process_changed_conditions(&mut self) {
        debug!("Conditions changed");
        // Invoke auto-load if necessary
        if self
            .basics
            .event_handler
            .auto_load_different_preset_if_necessary()
            .is_ok_and(|loaded| loaded)
        {
            // If another preset was loaded, we don't need to refresh all targets because
            // another preset is being loaded anyway.
            return;
        }
        // Refresh all targets
        debug!("Refreshing all targets...");
        for compartment in CompartmentKind::enum_iter() {
            let mut target_updates: Vec<RealTimeTargetUpdate> = vec![];
            let mut changed_mappings = vec![];
            let mut unused_sources = self.currently_feedback_enabled_sources(compartment, false);
            // Mappings with virtual targets don't have to be refreshed because virtual
            // targets are always active and never change depending on circumstances.
            for m in self.collections.mappings[compartment].values_mut() {
                let control_context = self.basics.control_context(compartment);
                let context = ExtendedProcessorContext::new(
                    &self.basics.context,
                    &self.collections.parameters,
                    control_context,
                );
                if let Some(target_update) = m.refresh_target(context, control_context) {
                    target_updates.push(target_update);
                    changed_mappings.push(m.id())
                }
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m
                        .source()
                        .extract_feedback_address(self.basics.source_context(m.compartment()))
                    {
                        unused_sources.remove(&addr);
                    }
                }
            }
            self.process_target_updates(compartment, target_updates);
            // Important to send IO event first ...
            self.notify_feedback_dev_usage_might_have_changed(compartment);
            self.handle_feedback_after_having_updated_particular_mappings(
                compartment,
                unused_sources,
                changed_mappings.into_iter(),
            );
        }
        // Update on mappings
        self.update_on_mappings();
        // Notify session (for UI purposes)
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::ConditionsChanged);
    }

    fn update_compartment_settings(
        &mut self,
        compartment: CompartmentKind,
        settings: CompartmentSettings,
    ) {
        self.basics.common_lua[compartment] = settings.common_lua;
    }

    fn update_settings(&mut self, settings: BasicSettings) {
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::UpdateSettings(settings));
        let any_main_mapping_is_effectively_on = self.any_main_mapping_is_effectively_on();
        self.basics
            .update_settings_internal(settings, any_main_mapping_is_effectively_on);
        self.potentially_enable_or_disable_control_or_feedback(any_main_mapping_is_effectively_on);
        Backbone::get()
            .register_stream_deck_usage(self.basics.unit_id, settings.streamdeck_device_id);
    }

    fn update_all_mappings(
        &mut self,
        compartment: CompartmentKind,
        mut mappings: Vec<MainMapping>,
    ) {
        tracing::debug!("Updating {} mappings in {}...", mappings.len(), compartment,);
        self.basics.clear_last_feedback();
        let mut mappings_by_group: NonCryptoHashMap<GroupId, Vec<MappingId>> = HashMap::default();
        let mut mapping_infos: NonCryptoHashMap<QualifiedMappingId, MappingInfo> =
            HashMap::default();
        let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
        self.collections.target_touch_dependent_mappings[compartment].clear();
        self.collections.beat_dependent_feedback_mappings[compartment].clear();
        self.collections.milli_dependent_feedback_mappings[compartment].clear();
        self.basics.target_based_conditional_activation_processors[compartment].clear();
        self.collections.previous_target_values[compartment].clear();
        self.poll_control_mappings[compartment].clear();
        // Refresh and splinter real-time mappings
        let real_time_mappings = mappings
            .iter_mut()
            .map(|m| {
                mappings_by_group
                    .entry(m.group_id())
                    .or_default()
                    .push(m.id());
                mapping_infos.insert(m.qualified_id(), m.take_mapping_info());
                let control_context = self.basics.control_context(m.compartment());
                m.init_target_and_activation(
                    ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    ),
                    control_context,
                );
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m
                        .source()
                        .extract_feedback_address(self.basics.source_context(m.compartment()))
                    {
                        unused_sources.remove(&addr);
                    }
                }
                if m.needs_refresh_when_target_touched() {
                    self.collections.target_touch_dependent_mappings[compartment].insert(m.id());
                }
                let feedback_resolution = m.feedback_resolution();
                if feedback_resolution == Some(FeedbackResolution::Beat) {
                    self.collections.beat_dependent_feedback_mappings[compartment].insert(m.id());
                }
                if feedback_resolution == Some(FeedbackResolution::High) {
                    self.collections.milli_dependent_feedback_mappings[compartment].insert(m.id());
                }
                if m.wants_to_be_polled_for_control() {
                    self.poll_control_mappings[compartment].insert(m.id());
                }
                let target_value_activation_reference_mappings =
                    m.activation_can_be_affected_by_target_values();
                self.basics.target_based_conditional_activation_processors[compartment]
                    .notify_usage_add_only(m.id(), target_value_activation_reference_mappings);
                m.splinter_real_time_mapping()
            })
            .collect();
        // Update instance state
        {
            let mut instance_state = self.basics.unit.borrow_mut();
            instance_state.set_mappings_by_group(compartment, mappings_by_group);
            instance_state.set_mapping_infos(mapping_infos);
        }
        // Put into hash map in order to quickly look up mappings by ID
        let mapping_tuples = mappings.into_iter().map(|m| (m.id(), m));
        if compartment == CompartmentKind::Controller {
            let (virtual_target_mappings, normal_mappings) =
                mapping_tuples.partition(|(_, m)| m.has_virtual_target());
            self.collections.mappings[compartment] = normal_mappings;
            self.collections.mappings_with_virtual_targets = virtual_target_mappings;
        } else {
            self.collections.mappings[compartment] = mapping_tuples.collect();
        }
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::UpdateAllMappings(
                compartment,
                real_time_mappings,
            ));
        // Important to send IO event first ...
        self.notify_feedback_dev_usage_might_have_changed(compartment);
        // ... and then mapping update. Otherwise, if this is an upper-floor instance
        // clearing all mappings, other instances won't see yet that they are actually
        // allowed to take over sources! Which might delay the reactivation of
        // lower-floor instances.
        self.handle_feedback_after_having_updated_all_mappings(compartment, unused_sources);
        self.update_on_mappings();
        // Evaluate target-based activation conditions. We do it by reporting
        // target value updates for all lead mappings.
        let lead_mapping_ids =
            self.basics.target_based_conditional_activation_processors[compartment].lead_mappings();
        self.process_conditional_activation_target_value_changes(compartment, lead_mapping_ids);
    }

    fn process_normal_tasks_from_real_time_processor(&mut self) {
        for task in self
            .basics
            .channels
            .normal_real_time_to_main_thread_task_receiver
            .try_iter()
            .take(NORMAL_RT_TASK_BULK_SIZE)
        {
            use NormalRealTimeToMainThreadTask::*;
            match task {
                CaptureMidi {
                    scan_result,
                    allow_virtual_sources,
                } => {
                    let event = MessageCaptureEvent {
                        result: MessageCaptureResult::Midi(scan_result),
                        allow_virtual_sources,
                        osc_arg_index_hint: None,
                    };
                    self.basics
                        .event_handler
                        .handle_event_ignoring_error(DomainEvent::CapturedIncomingMessage(event));
                }
                FullResyncToRealTimeProcessorPlease => {
                    // We cannot provide everything that the real-time processor needs so we need
                    // to delegate to the session in order to let it do the resync (could be
                    // changed by also holding unnecessary things but for now, why not taking the
                    // session detour).
                    self.basics
                        .event_handler
                        .handle_event_ignoring_error(DomainEvent::FullResyncRequested);
                }
                LogLifecycleOutput { value } => {
                    log_lifecycle_output(self.basics.unit_id, format_midi_source_value(&value));
                }
                LogToConsole(msg) => {
                    Reaper::get().show_console_msg(msg);
                }
            }
        }
    }

    fn any_main_mapping_is_effectively_on(&self) -> bool {
        self.collections.mappings[CompartmentKind::Main]
            .values()
            .any(|m| m.is_effectively_on())
    }

    fn notify_feedback_dev_usage_might_have_changed(&self, compartment: CompartmentKind) {
        // A device is only considered to be "in use" if there's at least one
        // *main* mapping. It doesn't depend on
        // controller mappings.
        if compartment == CompartmentKind::Main {
            let event = self.basics.feedback_output_usage_might_have_changed_event(
                self.any_main_mapping_is_effectively_on(),
            );
            debug!(
                "IO event. Feedback output used: {:?}",
                event.feedback_output_used
            );
            self.basics.send_io_update_complaining(event);
        }
    }

    fn send_io_update_if_space(&self, event: IoUpdatedEvent) {
        self.basics
            .channels
            .unit_orchestration_event_sender
            .send_if_space(UnitOrchestrationEvent::IoUpdated(event));
    }

    fn get_normal_or_virtual_target_mapping(
        &self,
        compartment: CompartmentKind,
        id: MappingId,
    ) -> Option<&MainMapping> {
        self.collections.mappings[compartment].get(&id).or(
            if compartment == CompartmentKind::Controller {
                self.collections.mappings_with_virtual_targets.get(&id)
            } else {
                None
            },
        )
    }

    fn get_normal_or_virtual_target_mapping_mut(
        &mut self,
        id: QualifiedMappingId,
    ) -> Option<&mut MainMapping> {
        self.collections.mappings[id.compartment]
            .get_mut(&id.id)
            .or(if id.compartment == CompartmentKind::Controller {
                self.collections
                    .mappings_with_virtual_targets
                    .get_mut(&id.id)
            } else {
                None
            })
    }

    pub fn process_reaper_config_changes(&self, changes: &[ReaperConfigChange]) {
        for change in changes {
            match change {
                ReaperConfigChange::ProjectOptions(_) => {
                    self.basics.channels.self_normal_sender.send_complaining(
                        NormalMainTask::PotentiallyEnableOrDisableControlOrFeedback,
                    );
                }
            }
        }
    }

    fn process_compartment_parameter_feedback(&mut self, param_index: PluginParamIndex) {
        let change_event = CompoundChangeEvent::CompartmentParameter(param_index);
        self.process_feedback_related_event(|mapping, target| {
            mapping.process_change_event(
                target,
                change_event,
                self.basics.control_context(mapping.compartment()),
            )
        });
    }

    pub fn process_additional_feedback_event(&mut self, event: &AdditionalFeedbackEvent) {
        if matches!(event, AdditionalFeedbackEvent::LastTouchedTargetChanged) {
            self.process_change_of_last_touched_target();
        }
        if let AdditionalFeedbackEvent::BeatChanged(_) = event {
            // This is fired very frequently so we don't want to iterate over all mappings,
            // just the ones that need to be notified for feedback or whatever.
            for compartment in CompartmentKind::enum_iter() {
                for mapping_id in
                    self.collections.beat_dependent_feedback_mappings[compartment].iter()
                {
                    if let Some(m) = self.collections.mappings[compartment].get(mapping_id) {
                        self.process_feedback_related_reaper_event_for_mapping(
                            m,
                            &mut |m, target| {
                                m.process_change_event(
                                    target,
                                    CompoundChangeEvent::Additional(event),
                                    self.basics.control_context(m.compartment()),
                                )
                            },
                        );
                    }
                }
            }
        } else {
            if ReaperTarget::changes_conditions(CompoundChangeEvent::Additional(event)) {
                self.basics.notify_conditions_changed();
            }
            // Okay, not fired that frequently, we can iterate over all mappings
            self.process_feedback_related_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::Additional(event),
                    self.basics.control_context(mapping.compartment()),
                )
            });
        }
    }

    pub fn process_control_surface_change_events(&self, events: &[ChangeEvent]) {
        if events.is_empty() {
            return;
        }
        // Potentially enable/disable control/feedback
        let influences_global_control_and_feedback = events.iter().any(|event| {
            match event {
                // Whenever ReaLearn is enabled/disabled via checkbox or dedicated API method.
                ChangeEvent::FxEnabledChanged(evt)
                    if &evt.fx == self.basics.context.containing_fx() =>
                {
                    true
                }
                // Whenever ReaLearn is enabled/disabled via Bypass parameter (e.g. via automation).
                ChangeEvent::FxParameterValueChanged(evt)
                    if evt.parameter.index() == self.basics.context.bypass_param_index()
                        && evt.parameter.fx() == self.basics.context.containing_fx() =>
                {
                    true
                }
                // Whenever ReaLearn's containing track is armed or disarmed.
                ChangeEvent::TrackArmChanged(evt)
                    if Some(&evt.track) == self.basics.context.containing_fx().track() =>
                {
                    true
                }
                // Whenever the project tab is switched.
                ChangeEvent::ProjectSwitched(_) => true,
                _ => false,
            }
        });
        if influences_global_control_and_feedback {
            self.basics
                .channels
                .self_normal_sender
                .send_complaining(NormalMainTask::PotentiallyEnableOrDisableControlOrFeedback);
        }
        // Refresh targets if necessary
        let we_have_a_potential_target_change_event = events
            .iter()
            .any(|evt| ReaperTarget::changes_conditions(CompoundChangeEvent::Reaper(evt)));
        if we_have_a_potential_target_change_event {
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
            self.basics.notify_conditions_changed();
        }
        // Process for feedback
        for event in events {
            self.process_feedback_related_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::Reaper(event),
                    self.basics.control_context(mapping.compartment()),
                )
            });
        }
    }

    /// The given function should return if the current target value is affected by this change
    /// and the new value. We do this because querying the value *immediately*
    /// using the target's `current_value()` method will in some or even many (?) cases give us the
    /// old value - which can lead to confusing feedback! In the past we unknowingly worked around
    /// this by deferring the value query to the next main cycle, but now that we have the nice
    /// non-rx change detection technique, we can do it right here, feedback without delay and
    /// avoid a redundant query.
    fn process_feedback_related_event(
        &self,
        mut f: impl Fn(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        for compartment in CompartmentKind::enum_iter() {
            // Mappings with virtual targets don't need to be considered here because they don't
            // cause feedback themselves.
            for m in self.collections.mappings[compartment].values() {
                self.process_feedback_related_reaper_event_for_mapping(m, &mut f);
            }
        }
    }

    /// The given function f is NOW required to return the current target value.
    fn process_feedback_related_reaper_event_for_mapping(
        &self,
        m: &MainMapping,
        f: &mut impl FnMut(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        self.basics
            .process_feedback_related_reaper_event_for_mapping(
                m,
                &self.collections.mappings_with_virtual_targets,
                f,
            );
    }

    fn wants_messages_in_general(&self) -> bool {
        match &self.basics.control_mode {
            ControlMode::Disabled => false,
            ControlMode::Controlling => self.basics.unit_control_is_effectively_enabled(),
            ControlMode::LearningSource { .. } => self.basics.control_is_globally_enabled,
        }
    }

    pub fn wants_keyboard_input(&self) -> bool {
        self.wants_messages_in_general() && self.basics.settings.wants_keyboard_input
    }

    pub fn wants_osc_from(&self, device_id: &OscDeviceId) -> bool {
        self.wants_messages_in_general()
            && self.basics.settings.control_input == ControlInput::Osc(*device_id)
    }

    pub fn process_info_event(&mut self, evt: &InternalInfoEvent) {
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::Info(evt));
    }

    pub fn process_reaper_message(&mut self, evt: ControlEvent<&ReaperMessage>) {
        // First process internally.
        // Convenience: Send all feedback whenever a MIDI device is connected.
        if let ReaperMessage::MidiDevicesConnected(payload) = evt.payload() {
            if let Some(FeedbackOutput::Midi(MidiDestination::Device(dev_id))) =
                self.basics.settings.feedback_output
            {
                if payload.output_devices.contains(&dev_id) {
                    self.basics
                        .channels
                        .self_normal_sender
                        .send_if_space(NormalMainTask::SendAllFeedback);
                }
            }
        }
        // Convenience: Send all feedback whenever a Stream Deck device is connected.
        if let ReaperMessage::StreamDeckDevicesConnected(payload) = evt.payload() {
            if let Some(dev_id) = self.basics.settings.streamdeck_device_id {
                if payload.devices.contains(&dev_id) {
                    self.basics
                        .channels
                        .self_normal_sender
                        .send_if_space(NormalMainTask::SendAllFeedback);
                }
            }
        }
        // Inform UI of MIDI device changes
        if matches!(
            evt.payload(),
            ReaperMessage::MidiDevicesConnected(_) | ReaperMessage::MidiDevicesDisconnected(_)
        ) {
            self.basics
                .event_handler
                .handle_event_ignoring_error(DomainEvent::MidiDevicesChanged);
        }
        // Then let mappings with REAPER sources process them, if controlling enabled.
        if self.basics.control_mode != ControlMode::Controlling {
            return;
        }
        if self.basics.settings.real_input_logging_enabled {
            log_real_control_input(self.basics.unit_id, evt);
        }
        if !self.basics.unit_control_is_effectively_enabled() {
            return;
        }
        let evt = evt.map_payload(MainSourceMessage::Reaper);
        let virtual_result = self
            .basics
            .process_controller_mappings_with_virtual_targets(
                &mut self.collections.mappings_with_virtual_targets,
                &mut self.collections.mappings[CompartmentKind::Main],
                evt,
                &self.collections.parameters,
            );
        for r in virtual_result.control_results {
            control_mapping_stage_three(
                &self.basics,
                &mut self.collections,
                r.compartment,
                r.control_result,
                GroupInteractionProcessing::On(r.group_interaction_input),
            )
        }
        self.process_mappings_with_real_targets(evt);
    }

    fn log_incoming_message<T: Display>(&self, msg: T) {
        match self.basics.control_mode {
            ControlMode::Controlling => {
                log_real_control_input(self.basics.unit_id, msg);
            }
            ControlMode::LearningSource { .. } => {
                log_real_learn_input(self.basics.unit_id, msg);
            }
            ControlMode::Disabled => {}
        }
    }

    /// This doesn't check if control enabled! You need to check before.
    pub fn process_incoming_key_msg(
        &mut self,
        evt: ControlEvent<KeyMessage>,
    ) -> KeyProcessingResult {
        if self.basics.settings.real_input_logging_enabled {
            self.log_incoming_message(evt);
        }
        let match_outcome =
            self.process_incoming_message_internal(evt.map_payload(MainSourceMessage::Key));
        // Because modifiers are processed as separate key events, not as compound shortcuts, the
        // following example works nicely: Cmd+B is let through to REAPER (= not filtered out) if there is an active
        // mapping listening to "Cmd" (a modifier!) but there is no active mapping with source 'B'. The 'B' will
        // not be consumed in this case and will be forwarded to REAPER as a compound shortcut.
        let filter_out_event = match_outcome.matched_or_consumed();
        KeyProcessingResult {
            match_outcome,
            filter_out_event,
        }
    }

    /// This doesn't check if control enabled! You need to check before.
    pub fn process_incoming_stream_deck_msg(&mut self, evt: ControlEvent<StreamDeckMessage>) {
        if self.basics.settings.real_input_logging_enabled {
            self.log_incoming_message(evt);
        }
        self.process_incoming_message_internal(evt.map_payload(MainSourceMessage::StreamDeck));
    }

    pub fn wants_stream_deck_input_from(&self, dev: StreamDeckDeviceId) -> bool {
        self.wants_messages_in_general()
            && self
                .basics
                .settings
                .streamdeck_device_id
                .is_some_and(|d| d == dev)
    }

    fn process_incoming_msg_for_controlling(
        &mut self,
        evt: ControlEvent<MainSourceMessage>,
    ) -> MatchOutcome {
        let virtual_result = self
            .basics
            .process_controller_mappings_with_virtual_targets(
                &mut self.collections.mappings_with_virtual_targets,
                &mut self.collections.mappings[CompartmentKind::Main],
                evt,
                &self.collections.parameters,
            );
        for r in virtual_result.control_results {
            control_mapping_stage_three(
                &self.basics,
                &mut self.collections,
                r.compartment,
                r.control_result,
                GroupInteractionProcessing::On(r.group_interaction_input),
            )
        }
        let real_match_outcome = self.process_mappings_with_real_targets(evt);
        virtual_result.match_outcome.merge_with(real_match_outcome)
    }

    /// This doesn't check if control enabled! You need to check before.
    pub fn process_incoming_osc_packet(&mut self, evt: ControlEvent<&OscPacket>) {
        if self.basics.settings.real_input_logging_enabled {
            let timestamp = evt.timestamp();
            self.log_incoming_message(ControlEvent::new(
                format_osc_packet(evt.into_payload()),
                timestamp,
            ));
        }
        match evt.payload() {
            OscPacket::Message(msg) => {
                let msg = MainSourceMessage::Osc(msg);
                self.process_incoming_message_internal(evt.with_payload(msg));
            }
            OscPacket::Bundle(bundle) => {
                for p in bundle.content.iter() {
                    self.process_incoming_osc_packet(evt.with_payload(p));
                }
            }
        }
    }

    fn process_incoming_message_internal(
        &mut self,
        evt: ControlEvent<MainSourceMessage>,
    ) -> MatchOutcome {
        match self.basics.control_mode {
            ControlMode::Controlling => self.process_incoming_msg_for_controlling(evt),
            ControlMode::LearningSource {
                allow_virtual_sources,
                osc_arg_index_hint,
            } => {
                if let Some(capture_result) = evt.payload().create_capture_result() {
                    self.process_incoming_msg_for_learning(
                        allow_virtual_sources,
                        osc_arg_index_hint,
                        capture_result,
                    );
                    MatchOutcome::Consumed
                } else {
                    // Should only happen with REAPER sources (where the match outcome isn't
                    // relevant).
                    MatchOutcome::Unmatched
                }
            }
            ControlMode::Disabled => {
                // "Disabled" means we use global learning, which is why we could consider it at
                // least as consumed ... normally. However, global learning for keyboard keys is not
                // supported yet, so we should not filter the event out! For OSC, we don't need the
                // info at all because OSC doesn't support filtering out events.
                MatchOutcome::Unmatched
            }
        }
    }

    fn process_incoming_msg_for_learning(
        &mut self,
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
        result: MessageCaptureResult,
    ) {
        let event = MessageCaptureEvent {
            result,
            allow_virtual_sources,
            osc_arg_index_hint,
        };
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::CapturedIncomingMessage(event));
    }

    /// Controls mappings with real targets in *both* compartments.
    fn process_mappings_with_real_targets(
        &mut self,
        evt: ControlEvent<MainSourceMessage>,
    ) -> MatchOutcome {
        let match_inactive = self.basics.settings.match_even_inactive_mappings;
        let mut match_outcome = MatchOutcome::Unmatched;
        for compartment in CompartmentKind::enum_iter() {
            let mut enforce_target_refresh = false;
            // Search for 958 to know why we use a for loop here instead of collect().
            let mut results = vec![];
            for m in self.collections.mappings[compartment]
                .values_mut()
                // Consider only control-enabled real mappings
                .filter(|m| m.core.options.control_is_enabled)
            {
                let mapping_is_active = m.is_active();
                if !mapping_is_active && !match_inactive {
                    continue;
                }
                let control_outcome = m.control_source(evt.payload());
                match_outcome.upgrade_from(control_outcome.into());
                if !mapping_is_active {
                    continue;
                }
                let control_value = match control_outcome {
                    Some(ControlOutcome::Matched(v)) => v,
                    _ => continue,
                };
                let control_event = evt.with_payload(control_value);
                let options = ControlOptions {
                    enforce_target_refresh,
                    ..Default::default()
                };
                let control_result = control_mapping_stage_one_and_two(
                    &self.basics,
                    &self.collections.parameters,
                    m,
                    control_event,
                    options,
                    ManualFeedbackProcessing::On {
                        mappings_with_virtual_targets: &self
                            .collections
                            .mappings_with_virtual_targets,
                    },
                );
                enforce_target_refresh = true;
                let extended_control_result = ExtendedMappingControlResult {
                    control_result,
                    compartment,
                    group_interaction_input: GroupInteractionInput {
                        mapping_id: m.id(),
                        group_interaction: m.group_interaction(),
                        control_event,
                    },
                };
                results.push(extended_control_result);
            }
            for r in results {
                control_mapping_stage_three(
                    &self.basics,
                    &mut self.collections,
                    r.compartment,
                    r.control_result,
                    GroupInteractionProcessing::On(r.group_interaction_input),
                )
            }
        }
        match_outcome
    }

    fn process_mapping_updates_due_to_activation_changes(
        &mut self,
        compartment: CompartmentKind,
        mapping_updates: Vec<RealTimeMappingUpdate>,
        target_updates: Vec<RealTimeTargetUpdate>,
        unused_sources: UnusedSources,
        changed_mappings: impl Iterator<Item = MappingId>,
    ) {
        // Send feedback
        self.handle_feedback_after_having_updated_particular_mappings(
            compartment,
            unused_sources,
            changed_mappings,
        );
        // Updating the "on" mappings is not exactly cheap as it turned out in
        // https://github.com/helgoboss/helgobox/issues/913. And it also turns out we can skip it in many cases
        // by checking if all updates are empty.
        if mapping_updates.is_empty() && target_updates.is_empty() {
            return;
        }
        // Propagate updates to real-time processor
        if !mapping_updates.is_empty() {
            self.basics
                .channels
                .normal_real_time_task_sender
                .send_complaining(NormalRealTimeTask::UpdateMappingsPartially(
                    compartment,
                    mapping_updates,
                ));
        }
        self.process_target_updates(compartment, target_updates);
        // Update on mappings
        self.update_on_mappings();
    }

    fn process_target_updates(
        &mut self,
        compartment: CompartmentKind,
        target_updates: Vec<RealTimeTargetUpdate>,
    ) {
        if target_updates.is_empty() {
            return;
        }
        // #692 If the target turns active/inactive or it changes to something else, we want it
        // to have an effect on target-based conditional activation.
        for target_update in &target_updates {
            if !self.basics.target_based_conditional_activation_processors[compartment]
                .is_lead_mapping(target_update.id)
            {
                continue;
            }
            let (target_is_active, target_value) =
                match self.collections.mappings[compartment].get(&target_update.id) {
                    None => continue,
                    Some(mapping) => {
                        let target_is_active = mapping.target_is_active();
                        let target_value = mapping.current_aggregated_target_value(
                            self.basics.control_context(mapping.compartment()),
                        );
                        (target_is_active, target_value)
                    }
                };
            let qualified_id = QualifiedMappingId::new(compartment, target_update.id);
            self.process_conditional_activation_target_value_change(
                qualified_id,
                target_is_active,
                target_value,
            );
        }
        // In some cases like closing projects, it's possible that this will
        // fail because the real-time processor is
        // already gone. But it doesn't matter.
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_if_space(NormalRealTimeTask::UpdateTargetsPartially(
                compartment,
                target_updates,
            ));
    }

    fn update_single_mapping_on_state(&self, id: QualifiedMappingId) {
        let is_on =
            if let Some(m) = self.get_normal_or_virtual_target_mapping(id.compartment, id.id) {
                m.is_effectively_on()
            } else {
                false
            };
        self.basics.event_handler.handle_event_ignoring_error(
            DomainEvent::UpdatedSingleMappingOnState(UpdatedSingleMappingOnStateEvent {
                id,
                is_on,
            }),
        );
    }

    fn update_on_mappings(&self) {
        let on_mappings = self
            .all_mappings()
            .filter(|m| m.is_effectively_on())
            .map(MainMapping::qualified_id)
            .collect();
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::UpdatedOnMappings(on_mappings));
    }

    fn send_feedback(
        &self,
        reason: FeedbackReason,
        feedback_values: impl IntoIterator<Item = CompoundFeedbackValue>,
    ) {
        self.basics.send_feedback(
            &self.collections.mappings_with_virtual_targets,
            reason,
            feedback_values,
        );
    }

    fn all_mappings(&self) -> impl Iterator<Item = &MainMapping> {
        self.all_mappings_without_virtual_targets()
            .chain(self.collections.mappings_with_virtual_targets.values())
    }

    /// Includes virtual mappings if the controller mapping compartment is queried.
    fn all_mappings_in_compartment(
        &self,
        compartment: CompartmentKind,
    ) -> impl Iterator<Item = &MainMapping> {
        self.collections.mappings[compartment].values().chain(
            self.collections
                .mappings_with_virtual_targets
                .values()
                // Include virtual target mappings if we are talking about controller compartment.
                .filter(move |_| compartment == CompartmentKind::Controller),
        )
    }

    fn all_mappings_without_virtual_targets(&self) -> impl Iterator<Item = &MainMapping> {
        all_mappings_without_virtual_targets(&self.collections.mappings)
    }

    pub fn send_all_feedback(&self) {
        self.basics.clear_last_feedback();
        self.send_feedback(FeedbackReason::Normal, self.feedback_all());
    }

    fn feedback_all(&self) -> Vec<CompoundFeedbackValue> {
        // Virtual targets don't cause feedback themselves
        self.all_mappings_without_virtual_targets()
            .filter_map(|m| {
                if m.feedback_is_effectively_on() {
                    m.feedback(true, self.basics.control_context(m.compartment()))
                } else {
                    None
                }
            })
            .collect()
    }

    fn feedback_particular_mappings(
        &self,
        compartment: CompartmentKind,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) -> Vec<CompoundFeedbackValue> {
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

    fn feedback_all_in_compartment(
        &self,
        compartment: CompartmentKind,
    ) -> Vec<CompoundFeedbackValue> {
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

    fn get_mapping_feedback_follow_virtual(
        &self,
        m: &MainMapping,
    ) -> Option<CompoundFeedbackValue> {
        let followed_mapping = self.follow_maybe_virtual_mapping(m)?;
        followed_mapping.feedback(
            true,
            self.basics.control_context(followed_mapping.compartment()),
        )
    }

    fn follow_maybe_virtual_mapping<'a>(&'a self, m: &'a MainMapping) -> Option<&'a MainMapping> {
        if let Some(control_element) = m.virtual_target_control_element() {
            find_active_main_mapping_connected_to_virtual_control_element(
                &self.collections.mappings[CompartmentKind::Main],
                control_element,
            )
        } else {
            Some(m)
        }
    }

    pub fn handle_change_of_some_upper_floor_instance(
        &self,
        feedback_output: DeviceFeedbackOutput,
    ) {
        self.update_on_mappings();
        if self
            .basics
            .settings
            .feedback_output
            .and_then(FeedbackOutput::device_output)
            == Some(feedback_output)
        {
            if self.basics.unit_feedback_is_effectively_enabled() {
                debug!("Reactivating unit...");
                // For this to really work reliably (eventual feedback consistency), it was
                // necessary to let the direct MIDI device feedback process in the global
                // *audio hook*, not in the real-time processor. Because there's only one audio
                // hook can guarantee a deterministic feedback send order.
                self.send_all_feedback();
            } else {
                debug!("Cancelling unit...");
                self.send_feedback(FeedbackReason::SuspendInstance, self.feedback_all_zero());
            }
        }
    }

    /// When feedback gets globally disabled.
    fn clear_all_feedback_allowing_source_takeover(&self) {
        debug!("Clearing all feedback allowing source takeover...");
        self.send_feedback(
            FeedbackReason::ClearAllAllowingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    pub fn switch_lights_off(&mut self) {
        if self.basics.unit_feedback_is_effectively_enabled() {
            // We clear feedback right here and now because that's the last chance.
            // Other instances can take over the feedback output afterwards.
            self.clear_all_feedback_preventing_source_takeover();
        }
    }

    /// When main processor goes away for good.
    fn clear_all_feedback_preventing_source_takeover(&self) {
        debug!("Clearing all feedback preventing source takeover...");
        self.send_feedback(
            FeedbackReason::ClearAllPreventingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    fn feedback_all_zero(&self) -> Vec<CompoundFeedbackValue> {
        // Mappings with virtual targets should not be included here because they might not be in
        // use and therefore should not *directly* send zeros. However, they will receive zeros
        // if one of the main mappings with virtual sources are connected to them.
        self.all_mappings_without_virtual_targets()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| {
                let logger = self.basics.source_feedback_logger(m.qualified_id());
                m.off_feedback(self.basics.source_context(m.compartment()), logger)
            })
            .collect()
    }

    /// Returns a hash map where each key is the source address of a source whose mapping
    /// emits feedback and the value is the "off feedback" message.
    ///
    /// This will be used to check which sources are not in use anymore and then send the
    /// "off feedback" message accordingly.
    fn currently_feedback_enabled_sources(
        &self,
        compartment: CompartmentKind,
        include_virtual: bool,
    ) -> UnusedSources {
        if include_virtual {
            self.all_mappings_in_compartment(compartment)
                .filter(|m| m.feedback_is_effectively_on())
                .filter_map(|m| {
                    Some((
                        m.source()
                            .extract_feedback_address(self.basics.source_context(compartment))?,
                        m.off_feedback(self.basics.source_context(compartment), NoopLogger)?,
                    ))
                })
                .collect()
        } else {
            self.collections.mappings[compartment]
                .values()
                .filter(|m| m.feedback_is_effectively_on())
                .filter_map(|m| {
                    Some((
                        m.source().extract_feedback_address(
                            self.basics.source_context(m.compartment()),
                        )?,
                        m.off_feedback(self.basics.source_context(m.compartment()), NoopLogger)?,
                    ))
                })
                .collect()
        }
    }

    fn handle_feedback_after_having_updated_all_mappings(
        &mut self,
        compartment: CompartmentKind,
        now_unused_sources: UnusedSources,
    ) {
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_all_in_compartment(compartment),
        );
        // It's important to send that *after* sending normal feedback since #660 because we might
        // have global source state (e.g. X-Touch Color state) and the order how we apply feedback
        // values to that global source state matters. It must correspond to the order in which
        // feedback messages are ultimately sent. "Off" feedback will always be sent *after* other
        // feedback because it's first subject to source takeover by other instances.
        self.send_off_feedback_for_unused_sources(now_unused_sources);
    }

    fn handle_feedback_after_having_updated_particular_mappings(
        &mut self,
        compartment: CompartmentKind,
        now_unused_sources: UnusedSources,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) {
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_particular_mappings(compartment, mapping_ids),
        );
        // It's important to send that *after* sending normal feedback since #660 because we might
        // have global source state (e.g. X-Touch Color state) and the order how we apply feedback
        // values to that global source state matters. It must correspond to the order in which
        // feedback messages are ultimately sent. "Off" feedback will always be sent *after* other
        // feedback because it's first subject to source takeover by other instances.
        self.send_off_feedback_for_unused_sources(now_unused_sources);
    }

    /// Indicate via off feedback the sources which are not in use anymore.
    fn send_off_feedback_for_unused_sources(&self, now_unused_sources: UnusedSources) {
        for feedback_value in now_unused_sources.into_values() {
            self.send_feedback(FeedbackReason::ClearUnusedSource, Some(feedback_value));
        }
    }

    fn log_debug_info(&mut self) {
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::LogDebugInfo);
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
            - Control task count: {} \n\
            - Feedback task count: {} \n\
            - Parameters: {:?} \n\
            ",
            self.basics.control_mode,
            self.collections.mappings[CompartmentKind::Main].len(),
            self.collections.mappings[CompartmentKind::Main]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.collections.mappings[CompartmentKind::Controller].len(),
            self.collections.mappings[CompartmentKind::Controller]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.collections.mappings_with_virtual_targets.len(),
            self.collections
                .mappings_with_virtual_targets
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.basics.channels.control_task_receiver.len(),
            self.basics.channels.feedback_task_receiver.len(),
            &self.collections.parameters,
        );
        Reaper::get().show_console_msg(msg);
        // Detailed
        trace!(
            "\n\
            # Main processor\n\
            \n\
            {:#?}
            ",
            self
        );
    }

    fn log_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::LogMapping(compartment, mapping_id));
        // Summary
        let mapping = self
            .all_mappings_in_compartment(compartment)
            .find(|m| m.id() == mapping_id);
        let msg = format!(
            "\n\
            # Main processor\n\
            \n\
            Mapping with ID {mapping_id}:\n\
            {mapping:#?}
            "
        );
        Reaper::get().show_console_msg(msg);
    }

    fn update_single_mapping(&mut self, mut mapping: Box<MainMapping>) {
        let compartment = mapping.compartment();
        debug!(
            "Updating single mapping {:?} in {}...",
            mapping.id(),
            compartment,
        );
        self.basics.clear_last_feedback();
        // Refresh
        let control_context = self.basics.control_context(compartment);
        mapping.init_target_and_activation(
            ExtendedProcessorContext::new(
                &self.basics.context,
                &self.collections.parameters,
                control_context,
            ),
            control_context,
        );
        let initial_target_value = mapping.initial_target_value();
        let lead_mapping_ids = mapping.activation_can_be_affected_by_target_values();
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::UpdateSingleMapping(
                compartment,
                Box::new(mapping.splinter_real_time_mapping()),
            ));
        // Update and feedback
        let id = QualifiedMappingId::new(compartment, mapping.id());
        // Important to do this before calculating diff feedback (because we might have
        // a textual feedback expression that contains the mapping name property).
        self.basics
            .unit
            .borrow_mut()
            .update_mapping_info(id, mapping.take_mapping_info());
        let diff_feedback = self.calc_diff_feedback_complicated(
            self.get_normal_or_virtual_target_mapping(mapping.compartment(), mapping.id()),
            &mapping,
        );
        let target_is_active = mapping.target_is_active();
        self.update_map_entries(compartment, *mapping);
        self.send_diff_feedback(diff_feedback);
        self.update_single_mapping_on_state(id);
        // This could be a lead mapping in terms of target-based conditional activation. If so,
        // the target value probably changed and all follow mappings can be affected.
        self.process_conditional_activation_target_value_change(
            id,
            target_is_active,
            initial_target_value,
        );
        // But it could also be a follow mapping.
        self.process_conditional_activation_target_value_changes(compartment, lead_mapping_ids);
    }

    fn process_conditional_activation_target_value_changes(
        &mut self,
        compartment: CompartmentKind,
        lead_mapping_ids: impl Iterator<Item = MappingId>,
    ) {
        for lead_mapping_id in lead_mapping_ids {
            let (target_is_active, target_value) =
                match self.collections.mappings[compartment].get(&lead_mapping_id) {
                    None => continue,
                    Some(mapping) => {
                        let target_is_active = mapping.target_is_active();
                        let target_value = mapping.current_aggregated_target_value(
                            self.basics.control_context(mapping.compartment()),
                        );
                        (target_is_active, target_value)
                    }
                };
            let qualified_id = QualifiedMappingId::new(compartment, lead_mapping_id);
            self.process_conditional_activation_target_value_change(
                qualified_id,
                target_is_active,
                target_value,
            );
        }
    }

    fn update_persistent_mapping_processing_state(
        &mut self,
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    ) {
        debug!(
            "Updating persistent processing state of mapping {:?} in {}",
            id.id, id.compartment
        );
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send_complaining(NormalRealTimeTask::UpdatePersistentMappingProcessingState {
                id,
                state,
            });
        // Update
        let (was_on_before, is_on_now) =
            if let Some(m) = self.get_normal_or_virtual_target_mapping_mut(id) {
                let was_on_before = m.feedback_is_effectively_on();
                m.update_persistent_processing_state(state);
                (was_on_before, m.feedback_is_effectively_on())
            } else {
                (false, false)
            };
        // Send feedback if necessary (right now we assume that changed processing state doesn't
        // change anything about the source or target, so we use a much more simple mechanism to
        // determine necessary diff feedback than when updating the complete mapping).
        if was_on_before != is_on_now {
            if let Some(m) = self.get_normal_or_virtual_target_mapping(id.compartment, id.id) {
                let fb = if is_on_now {
                    Fb::normal(self.get_mapping_feedback_follow_virtual(m))
                } else {
                    Fb::unused(
                        m.off_feedback(self.basics.source_context(m.compartment()), NoopLogger),
                    )
                };
                self.send_feedback(fb.0, fb.1);
            }
        }
        self.update_single_mapping_on_state(id);
    }

    /// Collect feedback (important to send later as soon as mappings updated).
    #[must_use]
    fn calc_diff_feedback_complicated(
        &self,
        previous_mapping: Option<&MainMapping>,
        mapping: &MainMapping,
    ) -> (Fb, Fb) {
        if let Some(previous_mapping) = previous_mapping {
            // An existing mapping is being overwritten.
            if previous_mapping.feedback_is_effectively_on() {
                // And its light is currently on.
                if mapping.source().has_same_feedback_address_as_source(
                    previous_mapping.source(),
                    self.basics.source_context(mapping.compartment()),
                ) {
                    // Source is the same.
                    if mapping.feedback_is_effectively_on() {
                        // Lights should still be on.
                        // Send new lights.
                        (
                            Fb::none(),
                            Fb::normal(self.get_mapping_feedback_follow_virtual(mapping)),
                        )
                    } else {
                        // Lights should now be off.
                        (
                            Fb::unused(mapping.off_feedback(
                                self.basics.source_context(mapping.compartment()),
                                NoopLogger,
                            )),
                            Fb::none(),
                        )
                    }
                } else {
                    // Source has changed.
                    // Switch previous source light off.
                    let fb1 = Fb::unused(previous_mapping.off_feedback(
                        self.basics.source_context(mapping.compartment()),
                        NoopLogger,
                    ));
                    let fb2 = if mapping.feedback_is_effectively_on() {
                        // Lights should be on. Send new lights.
                        Fb::normal(self.get_mapping_feedback_follow_virtual(mapping))
                    } else {
                        Fb::none()
                    };
                    // Unused *after* new (because sent in that order, important for #660)
                    (fb2, fb1)
                }
            } else {
                // Previous lights were off.
                if mapping.feedback_is_effectively_on() {
                    // Now should be on.
                    (
                        Fb::none(),
                        Fb::normal(self.get_mapping_feedback_follow_virtual(mapping)),
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
                    Fb::normal(self.get_mapping_feedback_follow_virtual(mapping)),
                )
            } else {
                // Lights off.
                (Fb::none(), Fb::none())
            }
        }
    }

    fn send_diff_feedback(&self, (fb1, fb2): (Fb, Fb)) {
        self.send_feedback(fb1.0, fb1.1);
        self.send_feedback(fb2.0, fb2.1);
    }

    fn update_map_entries(&mut self, compartment: CompartmentKind, m: MainMapping) {
        if m.needs_refresh_when_target_touched() {
            self.collections.target_touch_dependent_mappings[compartment].insert(m.id());
        } else {
            self.collections.target_touch_dependent_mappings[compartment].shift_remove(&m.id());
        }
        let influence = m.feedback_resolution();
        if influence == Some(FeedbackResolution::Beat) {
            self.collections.beat_dependent_feedback_mappings[compartment].insert(m.id());
        } else {
            self.collections.beat_dependent_feedback_mappings[compartment].shift_remove(&m.id());
        }
        if influence == Some(FeedbackResolution::High) {
            self.collections.milli_dependent_feedback_mappings[compartment].insert(m.id());
        } else {
            self.collections.milli_dependent_feedback_mappings[compartment].shift_remove(&m.id());
            self.collections.previous_target_values[compartment].remove(&m.id());
        }
        if m.wants_to_be_polled_for_control() {
            self.poll_control_mappings[compartment].insert(m.id());
        } else {
            self.poll_control_mappings[compartment].shift_remove(&m.id());
        }
        let target_value_activation_reference_mappings =
            m.activation_can_be_affected_by_target_values();
        self.basics.target_based_conditional_activation_processors[compartment]
            .notify_usage(m.id(), target_value_activation_reference_mappings);
        let relevant_map = if m.has_virtual_target() {
            self.collections.mappings[compartment].shift_remove(&m.id());
            &mut self.collections.mappings_with_virtual_targets
        } else {
            self.collections
                .mappings_with_virtual_targets
                .shift_remove(&m.id());
            &mut self.collections.mappings[compartment]
        };
        relevant_map.insert(m.id(), m);
    }

    fn hit_target(&mut self, id: QualifiedMappingId, value: ControlValue) {
        let control_result = if let Some(m) =
            self.collections.mappings[id.compartment].get_mut(&id.id)
        {
            let control_context = self.basics.control_context(id.compartment);
            let mut control_result = m.control_from_target_directly(
                control_context,
                ExtendedProcessorContext::new(
                    &self.basics.context,
                    &self.collections.parameters,
                    control_context,
                ),
                value,
                self.basics
                    .target_control_logger(ControlLogContext::Direct, m.qualified_id()),
            );
            control_mapping_stage_two(
                &self.basics,
                &mut control_result,
                m,
                ManualFeedbackProcessing::On {
                    mappings_with_virtual_targets: &self.collections.mappings_with_virtual_targets,
                },
            );
            Some(control_result)
        } else {
            None
        };
        if let Some(control_result) = control_result {
            control_mapping_stage_three(
                &self.basics,
                &mut self.collections,
                id.compartment,
                control_result,
                GroupInteractionProcessing::Off,
            );
        }
    }
}

/// State that contains only those properties of a mapping which ...
///
/// - make a difference in terms of processing
/// - are changed in response to processing
/// - and are persisted as part of the session.
///
/// These properties follow an unusual data flow, but still an unidirectional one: They are
/// propagated from the processing layer to the session (via synchronous event), persisted into the
/// session and sent back (asynchronously via channel) to the processor - which causes the actual
/// change.  
#[derive(Copy, Clone, Debug)]
pub struct PersistentMappingProcessingState {
    pub is_enabled: bool,
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(CompartmentKind, Vec<MainMapping>),
    /// Replaces the given mapping.
    // Boxed because much larger struct size than other variants.
    UpdateSingleMapping(Box<MainMapping>),
    // Available separately for performance reasons, because these updates are also triggered
    // triggered by processing itself, so it should happen fast.
    UpdatePersistentMappingProcessingState {
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    },
    /// Invokes the "ReaLearn instance started" source.
    NotifyRealearnUnitStarted,
    /// Invokes the "ReaLearn compartment loaded" source.
    NotifyRealearnCompartmentLoaded(CompartmentKind),
    /// Instructs the main processor to hit the target directly.
    ///
    /// This doesn't invoke group interaction because it's meant to totally skip the mode.
    HitTarget {
        id: QualifiedMappingId,
        value: ControlValue,
    },
    /// This should be sent on events such as track list change, FX focus etc.
    ///
    /// It will trigger a refresh of all targets (re-resolve) or even a preset change (if
    /// auto-load is enabled).
    NotifyConditionsChanged,
    UpdateSettings(BasicSettings),
    UpdateCompartmentSettings(CompartmentKind, CompartmentSettings),
    PotentiallyEnableOrDisableControlOrFeedback,
    SendAllFeedback,
    LogDebugInfo,
    LogMapping(CompartmentKind, MappingId),
    StartLearnSource {
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    },
    DisableControl,
    ReturnToControlMode,
    UseIntegrationTestFeedbackSender(SenderToNormalThread<FinalSourceFeedbackValue>),
}

#[derive(Copy, Clone, Debug, Default)]
pub struct BasicSettings {
    pub control_input: ControlInput,
    pub wants_keyboard_input: bool,
    pub streamdeck_device_id: Option<StreamDeckDeviceId>,
    pub feedback_output: Option<FeedbackOutput>,
    pub real_input_logging_enabled: bool,
    pub real_output_logging_enabled: bool,
    pub virtual_input_logging_enabled: bool,
    pub virtual_output_logging_enabled: bool,
    pub target_control_logging_enabled: bool,
    pub send_feedback_only_if_armed: bool,
    pub match_even_inactive_mappings: bool,
    pub let_matched_events_through: bool,
    pub let_unmatched_events_through: bool,
    pub reset_feedback_when_releasing_source: bool,
    pub stay_active_when_project_in_background: StayActiveWhenProjectInBackground,
}

#[derive(Clone, Debug, Default)]
pub struct CompartmentSettings {
    pub common_lua: Option<mlua::Value>,
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    derive_more::Display,
)]
pub enum StayActiveWhenProjectInBackground {
    /// Never.
    #[display(fmt = "Never")]
    Never,
    /// Respecting the REAPER project tab settings such as "Run background projects".
    #[display(fmt = "Only if background project is running")]
    OnlyIfBackgroundProjectIsRunning,
    /// As far as possible.
    #[display(fmt = "Always (more or less)")]
    Always,
}

impl Default for StayActiveWhenProjectInBackground {
    fn default() -> Self {
        Self::OnlyIfBackgroundProjectIsRunning
    }
}

impl BasicSettings {
    pub fn target_control_logger<'a>(
        &'a self,
        instance_state: &'a SharedUnit,
        context: ControlLogContext,
        mapping_id: QualifiedMappingId,
    ) -> impl Fn(ControlLogEntry) + 'a {
        move |entry| {
            if !self.target_control_logging_enabled {
                return;
            }
            if context == ControlLogContext::Polling
                && entry.error.is_empty()
                && entry.kind == ControlLogEntryKind::IgnoredByGlue
            {
                // This pollutes the log massively.
                return;
            }
            let instance_state = instance_state.borrow();
            let mapping_name = if let Some(info) = instance_state.get_mapping_info(mapping_id) {
                info.name.as_str()
            } else {
                "<unknown>"
            };
            log_target_control(
                instance_state.id(),
                format!("Mapping {mapping_name}: {entry} ({context})"),
            );
        }
    }
    /// For real-time processor usage.
    pub fn midi_control_input(&self) -> MidiControlInput {
        self.control_input
            .midi_control_input()
            .unwrap_or(MidiControlInput::FxInput)
    }

    /// For real-time processor usage.
    pub fn midi_destination(&self) -> Option<MidiDestination> {
        self.feedback_output
            .and_then(|output| output.midi_destination())
    }
}

/// A task which is sent from time to time from real-time to main processor.
#[derive(Debug)]
pub enum NormalRealTimeToMainThreadTask {
    CaptureMidi {
        scan_result: MidiScanResult,
        allow_virtual_sources: bool,
    },
    /// This is sent by the real-time processor after it has not been called for a while because
    /// the audio device was closed. It wants everything resynced:
    ///
    /// - All mappings
    /// - Instance settings
    /// - Feedback
    FullResyncToRealTimeProcessorPlease,
    LogLifecycleOutput {
        value: MidiSourceValue<'static, RawShortMessage>,
    },
    LogToConsole(String),
}

/// A parameter-related task (which is potentially sent very frequently, just think of automation).
#[derive(Debug)]
pub enum ParameterMainTask {
    UpdateSingleParamValue {
        index: PluginParamIndex,
        value: RawParamValue,
        timestamp: ControlEventTimestamp,
    },
    UpdateAllParams(PluginParams),
}

/// A feedback-related task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackMainTask {
    /// Only sent if this mapping is somewhere referenced in target value activation conditions.
    ///
    /// It's used for asynchronously re-evaluating the follow mappings' activation conditions.
    /// At this point, it can be assumed that the target is active!
    MappingTargetValueChanged {
        lead_mapping_id: QualifiedMappingId,
        target_value: AbsoluteValue,
    },
}

/// A control-related task (which is potentially sent very frequently).
pub enum ControlMainTask {
    /// Control event coming in from real-time processor (MIDI).
    ControlFromRealTime {
        compartment: CompartmentKind,
        mapping_id: MappingId,
        event: ControlEvent<ControlValue>,
        options: ControlOptions,
    },
    LogVirtualControlInput {
        event: ControlEvent<VirtualSourceValue>,
        match_outcome: MatchOutcome,
    },
    LogRealControlInput {
        event: ControlEvent<MidiSourceValue<'static, RawShortMessage>>,
        match_outcome: MatchOutcome,
    },
    LogRealLearnInput {
        event: ControlEvent<OwnedIncomingMidiMessage>,
    },
    LogTargetOutput {
        event: Box<RawMidiEvent>,
    },
    LogTargetControl {
        mapping_id: QualifiedMappingId,
        entry: ControlLogEntry,
    },
}

pub enum OwnedIncomingMidiMessage {
    Short(RawShortMessage),
    SysEx(Vec<u8>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ControlOptions {
    pub enforce_send_feedback_after_control: bool,
    pub mode_control_options: ModeControlOptions,
    /// Set this flag if this control operation is part of processing multiple mappings within one
    /// transaction.
    /// Reason: Possibly triggered change events (e.g. change of selected track) will result in
    /// refreshing all targets *after* the transaction, which might be to late if the user relies on
    /// mapping order! Setting `refresh_target` will enforce refreshing (without updating cache).
    pub enforce_target_refresh: bool,
    /// This means control was initiated in the real-time processor (currently MIDI only).
    ///
    /// This information is used by some particular targets whose work is partially done in real-time and partially
    /// in the main thread.
    pub coming_from_real_time: bool,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!("Dropping main processor...");
        self.switch_lights_off();
        let released_event = self
            .basics
            .io_released_event(self.any_main_mapping_is_effectively_on());
        self.send_io_update_if_space(released_event);
    }
}

/// Different feedback reasons can but don't have to result in slightly different behavior.
///
/// In any case, they are nice for tracing when debugging feedback issues.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FeedbackReason {
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

    /// When this returns true, it's feedback that's supposed to reset the source (fader, lights)
    /// because it has been released. This kind of feedback will only be sent if
    /// "Reset feedback when releasing source" is enabled.
    pub fn is_reset_because_of_source_release(self) -> bool {
        use FeedbackReason::*;
        matches!(
            self,
            ClearUnusedSource | ClearAllPreventingSourceTakeover | FinallySwitchOffSource
        )
    }
}

pub struct BasicSourceFeedbackLogger<'a, EH: DomainEventHandler> {
    basics: &'a Basics<EH>,
    mapping_id: QualifiedMappingId,
}

impl<EH: DomainEventHandler> SourceFeedbackLogger for BasicSourceFeedbackLogger<'_, EH> {
    fn log(&self, entry: FeedbackLogEntry) {
        let event = SourceFeedbackEvent {
            id: self.mapping_id,
            log_entry: entry,
        };
        self.basics
            .event_handler
            .handle_event_ignoring_error(DomainEvent::HandleSourceFeedback(event));
    }
}

impl<EH: DomainEventHandler> Basics<EH> {
    pub fn notify_conditions_changed(&self) {
        self.channels
            .self_normal_sender
            .send_complaining(NormalMainTask::NotifyConditionsChanged);
    }

    pub fn celebrate_success(&self) {
        self.event_handler
            .handle_event_ignoring_error(DomainEvent::TimeForCelebratingSuccess);
    }

    pub fn target_control_logger(
        &self,
        context: ControlLogContext,
        mapping_id: QualifiedMappingId,
    ) -> impl Fn(ControlLogEntry) + '_ {
        let console_logger = self
            .settings
            .target_control_logger(&self.unit, context, mapping_id);
        move |entry| {
            // Handle logging to mapping panel
            if context != ControlLogContext::Polling {
                let event = TargetControlEvent::new(mapping_id, context, entry);
                self.event_handler
                    .handle_event_ignoring_error(DomainEvent::HandleTargetControl(event));
            }
            // Handle logging to console
            console_logger(entry);
        }
    }

    pub fn source_context(&self, compartment: CompartmentKind) -> RealearnSourceContext {
        RealearnSourceContext {
            additional_script_input: AdditionalLuaMidiSourceScriptInput {
                compartment_lua: self.common_lua[compartment].as_ref(),
            },
        }
    }

    pub fn mode_context(&self, compartment: CompartmentKind) -> RealearnModeContext {
        RealearnModeContext {
            additional_script_input: AdditionalLuaFeedbackScriptInput {
                compartment_lua: self.common_lua[compartment].as_ref(),
            },
        }
    }

    pub fn source_feedback_logger(
        &self,
        mapping_id: QualifiedMappingId,
    ) -> impl SourceFeedbackLogger + '_ {
        BasicSourceFeedbackLogger {
            basics: self,
            mapping_id,
        }
    }

    pub fn update_settings_internal(
        &mut self,
        settings: BasicSettings,
        any_main_mapping_is_effectively_on: bool,
    ) {
        self.clear_last_feedback();
        // Send released event (important to create before updating settings)
        let released_event = self.io_released_event(any_main_mapping_is_effectively_on);
        self.send_io_update_complaining(released_event);
        // Update settings and feedback
        self.settings = settings;
    }

    pub fn potentially_enable_or_disable_control_internal(
        &mut self,
        any_main_mapping_is_effectively_on: bool,
        project_options: ProjectOptions,
    ) {
        let new_control_is_enabled =
            self.potentially_enable_or_disable_control_very_internal(project_options);
        // Important to send this to real-time processor even if state hasn't changed. It's possible that the
        // real-time processor has requested a full resync and therefore discarded a previous state change.
        self.channels.normal_real_time_task_sender.send_complaining(
            NormalRealTimeTask::UpdateControlIsGloballyEnabled(self.control_is_globally_enabled),
        );
        if let Some(new_control_is_enabled) = new_control_is_enabled {
            debug!(
                "Updated control_is_globally_enabled to {}",
                new_control_is_enabled
            );
            let event = IoUpdatedEvent {
                ..self.create_basic_io_changed_event(
                    any_main_mapping_is_effectively_on,
                    &self.current_control_feedback_settings(),
                )
            };
            self.send_io_update_complaining(event);
        }
    }

    /// Returns `Some` with new value if has actually changed.
    pub fn potentially_enable_or_disable_feedback_internal(
        &mut self,
        any_main_mapping_is_effectively_on: bool,
        project_options: ProjectOptions,
    ) -> Option<bool> {
        let new_feedback_is_enabled =
            self.potentially_enable_or_disable_feedback_very_internal(project_options);
        // Important to send this to real-time processor even if state hasn't changed. It's possible that the
        // real-time processor has requested a full resync and therefore discarded a previous state change.
        self.channels.normal_real_time_task_sender.send_complaining(
            NormalRealTimeTask::UpdateFeedbackIsGloballyEnabled(self.feedback_is_globally_enabled),
        );
        if let Some(new_feedback_is_enabled) = new_feedback_is_enabled {
            debug!(
                "Updated feedback_is_globally_enabled to {}",
                new_feedback_is_enabled
            );
            self.clear_last_feedback();
            let changed_event = self
                .feedback_output_usage_might_have_changed_event(any_main_mapping_is_effectively_on);
            self.send_io_update_complaining(changed_event);
        }
        new_feedback_is_enabled
    }

    /// Returns `Some` with new value if has actually changed.
    fn potentially_enable_or_disable_control_very_internal(
        &mut self,
        project_options: ProjectOptions,
    ) -> Option<bool> {
        let new_value =
            determine_control_globally_enabled(&self.context, &self.settings, project_options);
        let changed = new_value != self.control_is_globally_enabled;
        self.control_is_globally_enabled = new_value;
        if changed {
            Some(new_value)
        } else {
            None
        }
    }

    /// Returns `Some` with new value if has actually changed.
    fn potentially_enable_or_disable_feedback_very_internal(
        &mut self,
        project_options: ProjectOptions,
    ) -> Option<bool> {
        let new_value =
            determine_feedback_globally_enabled(&self.context, &self.settings, project_options);
        let changed = new_value != self.feedback_is_globally_enabled;
        self.feedback_is_globally_enabled = new_value;
        if changed {
            Some(new_value)
        } else {
            None
        }
    }

    fn send_io_update_complaining(&self, event: IoUpdatedEvent) {
        self.channels
            .unit_orchestration_event_sender
            .send_complaining(UnitOrchestrationEvent::IoUpdated(event));
    }

    fn io_released_event(&self, any_main_mapping_is_effectively_on: bool) -> IoUpdatedEvent {
        IoUpdatedEvent {
            control_input_used: false,
            feedback_output_used: false,
            ..self
                .feedback_output_usage_might_have_changed_event(any_main_mapping_is_effectively_on)
        }
    }

    fn current_control_feedback_settings(&self) -> ControlFeedbackSettings {
        ControlFeedbackSettings {
            control_input: self.settings.control_input,
            feedback_output: self.settings.feedback_output,
            control_is_globally_enabled: self.control_is_globally_enabled,
            feedback_is_globally_enabled: self.feedback_is_globally_enabled,
        }
    }

    fn feedback_output_usage_might_have_changed_event(
        &self,
        any_main_mapping_is_effectively_on: bool,
    ) -> IoUpdatedEvent {
        IoUpdatedEvent {
            feedback_output_usage_might_have_changed: true,
            ..self.create_basic_io_changed_event(
                any_main_mapping_is_effectively_on,
                &self.current_control_feedback_settings(),
            )
        }
    }

    fn create_basic_io_changed_event(
        &self,
        any_main_mapping_is_effectively_on: bool,
        settings: &ControlFeedbackSettings,
    ) -> IoUpdatedEvent {
        IoUpdatedEvent {
            unit_id: self.unit_id,
            control_input: settings.control_input.device_input(),
            control_input_used: settings.control_is_globally_enabled
                && any_main_mapping_is_effectively_on,
            feedback_output: settings.feedback_output.and_then(|o| o.device_output()),
            feedback_output_used: settings.feedback_is_globally_enabled
                && any_main_mapping_is_effectively_on,
            feedback_output_usage_might_have_changed: false,
        }
    }

    pub fn clear_last_feedback(&self) {
        self.last_feedback_checksum_by_address.borrow_mut().clear();
    }

    pub fn control_context(&self, compartment: CompartmentKind) -> ControlContext {
        ControlContext {
            feedback_audio_hook_task_sender: &self.channels.feedback_audio_hook_task_sender,
            feedback_real_time_task_sender: &self.channels.feedback_real_time_task_sender,
            osc_feedback_task_sender: &self.channels.osc_feedback_task_sender,
            feedback_output: self.settings.feedback_output,
            stream_deck_dev_id: self.settings.streamdeck_device_id,
            unit_container: self.unit_container,
            instance: &self.instance,
            unit: &self.unit,
            unit_id: self.unit_id,
            output_logging_enabled: self.settings.real_output_logging_enabled,
            source_context: self.source_context(compartment),
            mode_context: self.mode_context(compartment),
            processor_context: &self.context,
        }
    }

    pub fn process_group_interaction(
        &self,
        collections: &mut Collections,
        compartment: CompartmentKind,
        mapping_id: MappingId,
        control_event: ControlEvent<ControlValue>,
        control_was_successful: bool,
    ) {
        if let Some(m) = collections.mappings[compartment].get(&mapping_id) {
            // Group interaction
            let group_id = m.group_id();
            use GroupInteraction::*;
            match m.group_interaction() {
                None => {}
                SameControl | InverseControl => {
                    let control_value = if m.group_interaction().is_inverse() {
                        control_event.payload().inverse()
                    } else {
                        control_event.payload()
                    };
                    let control_event = control_event.with_payload(control_value);
                    self.process_other_mappings(
                        collections,
                        compartment,
                        mapping_id,
                        group_id,
                        |other_mapping, basics, parameters| {
                            let options = ControlOptions {
                                // Previous mappings in this transaction could affect
                                // subsequent mappings!
                                enforce_target_refresh: true,
                                ..Default::default()
                            };
                            control_mapping_stage_one_and_two(
                                basics,
                                parameters,
                                other_mapping,
                                control_event,
                                options,
                                ManualFeedbackProcessing::Off,
                            )
                        },
                    );
                }
                SameTargetValue
                | InverseTargetValue
                | InverseTargetValueOnOnly
                | InverseTargetValueOffOnly => {
                    if !control_was_successful {
                        return;
                    }
                    let context = self.control_context(compartment);
                    if let Some(reference_value) = m.current_aggregated_target_value(context) {
                        let is_on = reference_value.is_on();
                        if (m.group_interaction() == InverseTargetValueOnOnly && !is_on)
                            || (m.group_interaction() == InverseTargetValueOffOnly && is_on)
                        {
                            return;
                        }
                        let normalized_target_value = reference_value.normalize(
                            &m.mode().settings().target_value_interval,
                            &m.mode().settings().discrete_target_value_interval,
                            MinIsMaxBehavior::PreferOne,
                            m.mode().settings().use_discrete_processing,
                            BASE_EPSILON,
                        );
                        let inverse = m.group_interaction().is_inverse();
                        self.process_other_mappings(
                            collections,
                            compartment,
                            mapping_id,
                            group_id,
                            |other_mapping, basics, parameters| {
                                let control_context =
                                    basics.control_context(other_mapping.compartment());
                                other_mapping.control_from_target_via_group_interaction(
                                    normalized_target_value,
                                    ControlOptions {
                                        // Previous mappings in this transaction could affect
                                        // subsequent mappings!
                                        enforce_target_refresh: true,
                                        ..Default::default()
                                    },
                                    control_context,
                                    inverse,
                                    ExtendedProcessorContext::new(
                                        &self.context,
                                        parameters,
                                        control_context,
                                    ),
                                    basics.target_control_logger(
                                        ControlLogContext::GroupInteraction,
                                        other_mapping.qualified_id(),
                                    ),
                                )
                            },
                        );
                    }
                }
            }
        }
    }

    fn process_other_mappings(
        &self,
        collections: &mut Collections,
        compartment: CompartmentKind,
        mapping_id: MappingId,
        group_id: GroupId,
        f: impl Fn(&mut MainMapping, &Basics<EH>, &PluginParams) -> MappingControlResult,
    ) {
        let other_mappings = collections.mappings[compartment]
            .values_mut()
            .filter(|other_m| {
                other_m.id() != mapping_id
                    && other_m.group_id() == group_id
                    && other_m.control_is_effectively_on()
            });
        // Interestingly, we can't use closures like for_each or filter_map here in the same way
        // (fails with mutable + immutable borrow error). So we use a for loop and fill the
        // result vector manually.
        // TODO-low Rust question 958: Figure out the difference to the for loop.
        let mut hit_instructions = vec![];
        for other_mapping in other_mappings {
            let other_control_result = f(other_mapping, self, &collections.parameters);
            if let Some(new_value) = other_control_result.new_target_value {
                self.notify_target_value_changed(other_mapping, new_value);
            }
            self.send_feedback(
                &collections.mappings_with_virtual_targets,
                FeedbackReason::Normal,
                other_control_result.feedback_value,
            );
            if let Some(hi) = other_control_result.hit_instruction {
                hit_instructions.push(hi);
            }
        }
        for hi in hit_instructions {
            hi.execute(HitInstructionContext {
                mappings: &mut collections.mappings[compartment],
                control_context: self.control_context(compartment),
                domain_event_handler: &self.event_handler,
                basic_settings: &self.settings,
                processor_context: ExtendedProcessorContext::new(
                    &self.context,
                    &collections.parameters,
                    self.control_context(compartment),
                ),
            });
        }
    }

    /// The given function f is NOW required to return the current target value.
    // https://github.com/rust-lang/rust-clippy/issues/6066
    #[allow(clippy::needless_collect)]
    pub fn process_feedback_related_reaper_event_for_mapping(
        &self,
        m: &MainMapping,
        mappings_with_virtual_targets: &OrderedMappingMap<MainMapping>,
        f: &mut impl FnMut(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        // It's enough if one of the resolved targets is affected. Then we are going to need the
        // values of all of them!
        let mut at_least_one_target_is_affected = false;
        let new_values = m.targets().iter().filter_map(|target| {
            let target = match target {
                CompoundMappingTarget::Reaper(t) => t,
                _ => return None,
            };
            // Immediate value capturing. Makes OSC feedback *much* smoother in
            // combination with high-throughput thread. Especially quick pulls
            // of many faders at once profit from it because intermediate
            // values are captured and immediately sent so user doesn't see
            // stuttering faders on their device.
            // It's important to capture the current value from the event because
            // querying *at this time* from the target itself might result in
            // the old value to be returned. This is the case with FX parameter
            // changes for examples and especially in case of on/off targets this
            // can lead to horribly wrong feedback. Previously we didn't have this
            // issue because we always deferred to the next main loop cycle.
            let (value_changed, new_value) = f(m, target);
            if value_changed {
                at_least_one_target_is_affected = true;
            }
            Some(new_value)
        });
        let new_target_value = aggregate_target_values(new_values);
        if !at_least_one_target_is_affected {
            return;
        }
        // Feedback
        let mapping_feedback_is_effectively_on = m.feedback_is_effectively_on();
        let with_projection_feedback = mapping_feedback_is_effectively_on;
        let with_source_feedback =
            self.unit_feedback_is_effectively_enabled() && mapping_feedback_is_effectively_on;
        let logger = self.source_feedback_logger(m.qualified_id());
        let feedback_value = m.feedback_entry_point(
            FeedbackDestinations {
                with_projection_feedback,
                with_source_feedback,
            },
            new_target_value,
            self.control_context(m.compartment()),
            logger,
        );
        self.send_feedback(
            mappings_with_virtual_targets,
            FeedbackReason::Normal,
            feedback_value,
        );
        if let Some(v) = new_target_value {
            self.notify_target_value_changed(m, v);
        }
    }

    /// Inform session, e.g. for UI updates, but also for target-based conditional activation.
    fn notify_target_value_changed(&self, m: &MainMapping, new_value: AbsoluteValue) {
        self.process_target_value_change_for_conditional_activation(m.qualified_id(), new_value);
        self.notify_session_about_target_value_change(m, new_value);
    }

    fn process_target_value_change_for_conditional_activation(
        &self,
        mapping_id: QualifiedMappingId,
        new_value: AbsoluteValue,
    ) {
        // Defer evaluation of target-based activation conditions (defer because we don't have
        // mutable access at this point).
        if self.target_based_conditional_activation_processors[mapping_id.compartment]
            .is_lead_mapping(mapping_id.id)
        {
            let task = FeedbackMainTask::MappingTargetValueChanged {
                lead_mapping_id: mapping_id,
                target_value: new_value,
            };
            self.channels.self_feedback_sender.send_complaining(task);
        }
    }

    fn notify_session_about_target_value_change(&self, m: &MainMapping, new_value: AbsoluteValue) {
        let event = DomainEvent::TargetValueChanged(TargetValueChangedEvent {
            compartment: m.compartment(),
            mapping_id: m.id(),
            targets: m.targets(),
            new_value,
        });
        self.event_handler.handle_event_ignoring_error(event);
    }

    /// Processes (controller) mappings with virtual targets.
    ///
    /// This also includes controlling the (main) mappings with corresponding virtual sources.
    #[must_use]
    pub fn process_controller_mappings_with_virtual_targets(
        &self,
        mappings_with_virtual_targets: &mut OrderedMappingMap<MainMapping>,
        // Contains mappings with virtual sources
        main_mappings: &mut OrderedMappingMap<MainMapping>,
        evt: ControlEvent<MainSourceMessage>,
        params: &PluginParams,
    ) -> ProcessVirtualResult {
        // Control
        let mut match_outcome = MatchOutcome::Unmatched;
        let mut extended_control_results: Vec<_> = mappings_with_virtual_targets
            .values_mut()
            // For mappings with virtual targets, the setting "Match even inactive mappings" isn't relevant.
            // If such mappings are inactive, they will never be considered as matched. Only associated main mappings
            // decide about the match result.
            .filter(|m| m.control_is_effectively_on())
            .flat_map(|m| {
                let virtual_source_value = match m.control_virtualizing(evt) {
                    // When matched, the existence of associated main mappings will decide over the match result!
                    Some(ControlOutcome::Matched(v)) => v,
                    // When unmatched or consumed, don't process further, but at least update match result.
                    // I was wondering if it's inconsequential to allow virtual mappings without associated
                    // main mappings be able to *consume* an event. I mean, even a *matching* virtual mapping
                    // with associated main mappings might not get that power! But I came to the conclusion that it's
                    // consequential. "Consume" means that e.g. an NRPN source eats an event which contains only
                    // a partial message, to check if later events lead to the *complete* message. In the real-time
                    // processor, we have the same behavior. It's not relevant whether the mapping actually leads to
                    // some action or not.
                    unmatched_or_consumed => {
                        match_outcome.upgrade_from(unmatched_or_consumed.into());
                        return vec![];
                    }
                };
                self.event_handler
                    .notify_mapping_matched(CompartmentKind::Controller, m.id());
                let virtual_result = self.process_main_mappings_with_virtual_sources(
                    main_mappings,
                    evt.with_payload(virtual_source_value),
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
                        enforce_send_feedback_after_control: m.options().feedback_send_behavior
                            == FeedbackSendBehavior::SendFeedbackAfterControl,
                        mode_control_options: m.mode_control_options(),
                        // Not yet important at this point because one virtual target can't
                        // affect a subsequent one.
                        enforce_target_refresh: false,
                        // If we are here, we know control originated in main thread, not in real-time
                        coming_from_real_time: false,
                    },
                    params,
                );
                match_outcome.upgrade_from(virtual_result.match_outcome);
                if self.settings.virtual_input_logging_enabled {
                    log_virtual_control_input(
                        self.unit_id,
                        format_control_input_with_match_result(
                            virtual_source_value,
                            virtual_result.match_outcome,
                        ),
                    );
                }
                virtual_result.control_results
            })
            .collect();
        // Feedback
        self.send_feedback(
            mappings_with_virtual_targets,
            FeedbackReason::Normal,
            extended_control_results
                .iter_mut()
                .filter_map(|r| r.control_result.feedback_value.take()),
        );
        ProcessVirtualResult {
            control_results: extended_control_results,
            match_outcome,
        }
    }

    /// Sends both direct and virtual-source feedback.
    pub fn send_feedback(
        &self,
        mappings_with_virtual_targets: &OrderedMappingMap<MainMapping>,
        feedback_reason: FeedbackReason,
        feedback_values: impl IntoIterator<Item = CompoundFeedbackValue>,
    ) {
        let mut global_source_state = Backbone::source_state().borrow_mut();
        let mut feedback_collector =
            FeedbackCollector::new(&mut global_source_state, self.settings.feedback_output);
        for feedback_value in feedback_values.into_iter() {
            match feedback_value.value {
                SpecificCompoundFeedbackValue::Virtual {
                    destinations,
                    value,
                } => {
                    // At this point we still include controller mappings for which feedback
                    // is explicitly not enabled (not supported by controller) in order to
                    // support at least projection feedback (#414)!
                    if self.settings.virtual_output_logging_enabled {
                        log_virtual_feedback_output(self.unit_id, &value);
                    }
                    // Iterate over (controller) mappings with virtual targets.
                    for m in mappings_with_virtual_targets
                        .values()
                        .filter(|m| m.feedback_is_effectively_on())
                    {
                        // Should always be true.
                        if let Some(t) = m.virtual_target() {
                            if t.control_element() == value.control_element() {
                                // Virtual source matched virtual target. The following method
                                // will always produce real target values (because controller
                                // mappings can't have virtual sources).
                                let compound_feedback_value = m.feedback_given_target_value(
                                    // This clone is unavoidable because we are producing
                                    // real feedback values and these will be sent to another
                                    //  thread, so they must be self-contained.
                                    Some(Cow::Borrowed(value.feedback_value())),
                                    FeedbackDestinations {
                                        with_source_feedback: destinations.with_source_feedback
                                            && m.feedback_is_enabled(),
                                        ..destinations
                                    },
                                    self.source_context(m.compartment()),
                                    self.source_feedback_logger(m.qualified_id()),
                                    feedback_value.cause,
                                );
                                if let Some(SpecificCompoundFeedbackValue::Real(
                                    preliminary_feedback_value,
                                )) = compound_feedback_value
                                {
                                    // Successful virtual-to-real feedback
                                    if let Some(final_feedback_value) =
                                        feedback_collector.process(preliminary_feedback_value)
                                    {
                                        if let Some(t) = &m.mode().settings().control_transformation
                                        {
                                            if let Some(numeric_fb_value) =
                                                value.feedback_value().to_numeric()
                                            {
                                                t.set_last_feedback_value(
                                                    numeric_fb_value.value.to_unit_value().get(),
                                                );
                                            }
                                        }
                                        self.send_direct_feedback(
                                            feedback_reason,
                                            feedback_value.cause,
                                            final_feedback_value,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                SpecificCompoundFeedbackValue::Real(preliminary_feedback_value) => {
                    if let Some(final_feedback_value) =
                        feedback_collector.process(preliminary_feedback_value)
                    {
                        self.send_direct_feedback(
                            feedback_reason,
                            feedback_value.cause,
                            final_feedback_value,
                        );
                    }
                }
            }
        }
        // Send special collected feedback
        for final_feedback_value in feedback_collector.generate_final_feedback_values() {
            self.send_direct_feedback(feedback_reason, FeedbackCause::Normal, final_feedback_value);
        }
    }

    pub fn send_direct_source_feedback(
        &self,
        output: Option<FeedbackOutput>,
        reason: FeedbackReason,
        cause: FeedbackCause,
        source_feedback_value: FinalSourceFeedbackValue,
    ) {
        if reason.is_reset_because_of_source_release()
            && !self.settings.reset_feedback_when_releasing_source
        {
            return;
        }
        // Record feedback checksum and block duplicate feedback in some cases.
        if let Some(address) = source_feedback_value.extract_address() {
            // Extracting a feedback address is not super cheap for OSC and MIDI Raw because it has to
            // clone the address string. On the other hand, address strings are not large, so what.
            let checksum = FeedbackChecksum::from_value(&source_feedback_value);
            let previous_checksum = self
                .last_feedback_checksum_by_address
                .borrow_mut()
                .insert(address, checksum);
            // Block duplicates in certain cases. If it's feedback-after-control, we always need to
            // send because that's sort of the point of this feature. If it's a source-takeover, we
            // also need to send because we don't know what the other instance sent before that
            // (https://github.com/helgoboss/helgobox/issues/727).
            if cause != FeedbackCause::FeedbackAfterControl
                && reason != FeedbackReason::TakeOverSource
                && Some(checksum) == previous_checksum
            {
                trace!(
                    "Block feedback because duplicate (reason: {:?}): {:?}",
                    reason,
                    source_feedback_value
                );
                return;
            }
        }
        trace!(
            "Schedule sending feedback because {:?}: {:?}",
            reason,
            source_feedback_value
        );
        if let Some(test_sender) = self.channels.integration_test_feedback_sender.as_ref() {
            // Integration test
            // Test receiver could already be gone (if the test didn't wait long enough).
            test_sender.send_if_space(source_feedback_value);
        } else {
            // Production
            match (source_feedback_value, output) {
                (FinalSourceFeedbackValue::Midi(v), Some(FeedbackOutput::Midi(midi_output))) => {
                    match midi_output {
                        MidiDestination::FxOutput => {
                            if self.settings.real_output_logging_enabled {
                                log_real_feedback_output(
                                    self.unit_id,
                                    reason,
                                    format_midi_source_value(&v),
                                );
                            }
                            self.channels
                                .feedback_real_time_task_sender
                                .send_complaining(FeedbackRealTimeTask::FxOutputFeedback(v));
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
                            if self.settings.real_output_logging_enabled {
                                log_real_feedback_output(
                                    self.unit_id,
                                    reason,
                                    format_midi_source_value(&v),
                                );
                            }
                            self.channels
                                .feedback_audio_hook_task_sender
                                .send_complaining(FeedbackAudioHookTask::MidiDeviceFeedback(
                                    dev_id, v,
                                ));
                        }
                    }
                }
                (FinalSourceFeedbackValue::Osc(msg), Some(FeedbackOutput::Osc(dev_id))) => {
                    if self.settings.real_output_logging_enabled {
                        log_real_feedback_output(self.unit_id, reason, format_osc_message(&msg));
                    }
                    self.channels
                        .osc_feedback_task_sender
                        .send_complaining(OscFeedbackTask::new(dev_id, msg));
                }
                (FinalSourceFeedbackValue::Reaper(ReaperSourceFeedbackValue::Speech(v)), _) => {
                    let _ = say(v);
                }
                (FinalSourceFeedbackValue::StreamDeck(v), _) => {
                    if let Some(dev_id) = self.settings.streamdeck_device_id {
                        let _ = Backbone::get().send_stream_deck_feedback(dev_id, v);
                    }
                }
                _ => {}
            }
        }
    }

    fn send_direct_feedback(
        &self,
        reason: FeedbackReason,
        cause: FeedbackCause,
        value: FinalRealFeedbackValue,
    ) {
        self.send_direct_device_feedback(reason, cause, value.source);
        self.send_direct_projection_feedback(value.projection);
    }

    fn send_direct_projection_feedback(&self, feedback_value: Option<ProjectionFeedbackValue>) {
        if let Some(projection_feedback_value) = feedback_value {
            self.event_handler
                .handle_event_ignoring_error(DomainEvent::ProjectionFeedback(
                    projection_feedback_value,
                ));
        }
    }

    fn send_direct_device_feedback(
        &self,
        reason: FeedbackReason,
        cause: FeedbackCause,
        value: Option<FinalSourceFeedbackValue>,
    ) {
        if !reason.is_always_allowed() && !self.unit_feedback_is_effectively_enabled() {
            return;
        }
        if let Some(source_feedback_value) = value {
            // At this point we can be sure that this mapping can't have a
            // virtual source.
            if reason.is_source_release() {
                // Possible interference with other instances. Don't switch off yet!
                // Give other instances the chance to take over.
                let event = UnitOrchestrationEvent::SourceReleased(SourceReleasedEvent {
                    unit_id: self.unit_id.to_owned(),
                    feedback_output: self.settings.feedback_output,
                    feedback_value: source_feedback_value,
                });
                self.channels
                    .unit_orchestration_event_sender
                    .send_complaining(event);
            } else {
                // Send feedback right now.
                self.send_direct_source_feedback(
                    self.settings.feedback_output,
                    reason,
                    cause,
                    source_feedback_value,
                );
            }
        }
    }

    pub fn unit_control_is_effectively_enabled(&self) -> bool {
        self.control_is_globally_enabled
            && Backbone::get().control_is_allowed(&self.unit_id, self.settings.control_input)
    }

    pub fn unit_feedback_is_effectively_enabled(&self) -> bool {
        if !self.feedback_is_globally_enabled {
            return false;
        }
        if let Some(fo) = self.settings.feedback_output {
            // Feedback output is set
            Backbone::get().feedback_is_allowed(&self.unit_id, fo)
        } else {
            // Pointless but allowed
            true
        }
    }

    /// Processes main mappings with virtual sources.
    ///
    /// Returns a list of all invocation results. Empty if no mapping matched at all.
    fn process_main_mappings_with_virtual_sources(
        &self,
        main_mappings: &mut OrderedMappingMap<MainMapping>,
        evt: ControlEvent<VirtualSourceValue>,
        options: ControlOptions,
        params: &PluginParams,
    ) -> ProcessVirtualResult {
        // Controller mappings can't have virtual sources, so for now we only need to check
        // main mappings.
        let mut enforce_target_refresh = false;
        let match_inactive = self.settings.match_even_inactive_mappings;
        let mut match_outcome = MatchOutcome::Unmatched;
        let control_results = main_mappings
            .values_mut()
            // Consider only control-enabled main mappings
            .filter(|m| m.core.options.control_is_enabled)
            .filter_map(|m| {
                let mapping_is_active = m.is_active();
                if !mapping_is_active && !match_inactive {
                    return None;
                }
                let CompoundMappingSource::Virtual(s) = &m.source() else {
                    return None;
                };
                let control_value = s.control(&evt.payload())?;
                // We found an associated main mapping, so it's not just consumed, it's matched.
                match_outcome = MatchOutcome::Matched;
                if !mapping_is_active {
                    return None;
                }
                let control_event = evt.with_payload(control_value);
                let options = ControlOptions {
                    enforce_target_refresh,
                    ..options
                };
                let control_result = control_mapping_stage_one_and_two(
                    self,
                    params,
                    m,
                    control_event,
                    options,
                    ManualFeedbackProcessing::Off,
                );
                enforce_target_refresh = true;
                let extended_control_result = ExtendedMappingControlResult {
                    control_result,
                    compartment: m.compartment(),
                    group_interaction_input: GroupInteractionInput {
                        mapping_id: m.id(),
                        group_interaction: m.group_interaction(),
                        control_event,
                    },
                };
                Some(extended_control_result)
            })
            .collect();
        ProcessVirtualResult {
            control_results,
            match_outcome,
        }
    }
}

/// Includes virtual mappings if the controller mapping compartment is queried.
fn all_mappings_in_compartment_mut<'a>(
    mappings: &'a mut EnumMap<CompartmentKind, OrderedMappingMap<MainMapping>>,
    mappings_with_virtual_targets: &'a mut OrderedMappingMap<MainMapping>,
    compartment: CompartmentKind,
) -> impl Iterator<Item = &'a mut MainMapping> {
    mappings[compartment].values_mut().chain(
        mappings_with_virtual_targets
            .values_mut()
            // Include virtual target mappings if we are talking about controller compartment.
            .filter(move |_| compartment == CompartmentKind::Controller),
    )
}

fn get_normal_or_virtual_target_mapping_mut<'a>(
    mappings: &'a mut EnumMap<CompartmentKind, OrderedMappingMap<MainMapping>>,
    mappings_with_virtual_targets: &'a mut OrderedMappingMap<MainMapping>,
    compartment: CompartmentKind,
    id: MappingId,
) -> Option<&'a mut MainMapping> {
    mappings[compartment]
        .get_mut(&id)
        .or(if compartment == CompartmentKind::Controller {
            mappings_with_virtual_targets.get_mut(&id)
        } else {
            None
        })
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct UnitId(u32);

impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl UnitId {
    pub fn next() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl From<u32> for UnitId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<UnitId> for u32 {
    fn from(value: UnitId) -> Self {
        value.0
    }
}

impl From<ControlUnitId> for UnitId {
    fn from(value: ControlUnitId) -> Self {
        UnitId(value.get())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Display)]
pub enum MatchOutcome {
    /// Message didn't match at all, was not even consumed.
    #[display(fmt = "unmatched")]
    Unmatched = 0,
    /// Message was not processed but at least consumed (= "eaten").
    ///
    /// That means it should not be forwarded if "Let matched events through" is `false`.
    #[display(fmt = "consumed")]
    Consumed = 1,
    /// Message was processed.
    ///
    /// That means it should not be forwarded if "Let matched events through" is `false`.
    #[display(fmt = "matched")]
    Matched = 2,
}

impl MatchOutcome {
    pub fn matched_or_consumed(&self) -> bool {
        matches!(self, Self::Matched | Self::Consumed)
    }

    pub fn matched(&self) -> bool {
        matches!(self, Self::Matched)
    }

    pub fn merge_with(self, rhs: MatchOutcome) -> MatchOutcome {
        [self, rhs].into_iter().max().unwrap()
    }

    pub fn upgrade_from(&mut self, rhs: MatchOutcome) {
        if rhs > *self {
            *self = rhs;
        }
    }
}

impl<T> From<Option<ControlOutcome<T>>> for MatchOutcome {
    fn from(v: Option<ControlOutcome<T>>) -> Self {
        match v {
            None => Self::Unmatched,
            Some(ControlOutcome::Consumed) => MatchOutcome::Consumed,
            Some(ControlOutcome::Matched(_)) => MatchOutcome::Matched,
        }
    }
}

#[must_use]
fn control_mapping_stage_one_and_two<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    params: &PluginParams,
    m: &mut MainMapping,
    control_event: ControlEvent<ControlValue>,
    options: ControlOptions,
    feedback_handling: ManualFeedbackProcessing,
) -> MappingControlResult {
    let mut control_result = control_mapping_stage_one(basics, params, m, control_event, options);
    control_mapping_stage_two(basics, &mut control_result, m, feedback_handling);
    control_result
}

/// Executes stage one of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Notifying that mapping matched
/// 2. Controlling with given control value (probably produced by source) starting from mode.
#[must_use]
fn control_mapping_stage_one<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    params: &PluginParams,
    m: &mut MainMapping,
    control_event: ControlEvent<ControlValue>,
    options: ControlOptions,
) -> MappingControlResult {
    basics
        .event_handler
        .notify_mapping_matched(m.compartment(), m.id());
    let result = m.control_from_mode(
        control_event,
        options,
        basics.control_context(m.compartment()),
        ExtendedProcessorContext::new(
            &basics.context,
            params,
            basics.control_context(m.compartment()),
        ),
        m.last_non_performance_target_value(),
        basics.target_control_logger(ControlLogContext::Normal, m.qualified_id()),
    );
    if result.at_least_one_target_caused_effect && result.celebrate_success {
        basics.celebrate_success();
    }
    result
}

/// Executes stage two of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Sending manual feedback due to target or "Send feedback after control".
fn control_mapping_stage_two<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    control_result: &mut MappingControlResult,
    m: &mut MainMapping,
    feedback_handling: ManualFeedbackProcessing,
) {
    if let Some(new_value) = control_result.new_target_value {
        basics.notify_target_value_changed(m, new_value);
    }
    if let ManualFeedbackProcessing::On {
        mappings_with_virtual_targets,
    } = feedback_handling
    {
        basics.send_feedback(
            mappings_with_virtual_targets,
            FeedbackReason::Normal,
            control_result.feedback_value.take(),
        );
    }
}

/// Executes stage three of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Executing a possible hit instruction (and in a possible second pass all resulting hit
///    instructions). A second pass is not just theory, it makes a lot of sense in practice, e.g.
///    when we control "Enable/disable mappings" via "Browse group mappings". However, we should
///    stop there in order to prevent infinite loops. If we really need more in future, we can add
///    a third pass.  
/// 2. Processing group interaction (if enabled).
fn control_mapping_stage_three<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    collections: &mut Collections,
    compartment: CompartmentKind,
    control_result: MappingControlResult,
    group_interaction_processing: GroupInteractionProcessing,
) {
    if let Some(hi) = control_result.hit_instruction {
        let control_context = basics.control_context(compartment);
        let processor_context = ExtendedProcessorContext::new(
            &basics.context,
            &collections.parameters,
            control_context,
        );
        let response = hi.execute(HitInstructionContext {
            mappings: &mut collections.mappings[compartment],
            control_context,
            domain_event_handler: &basics.event_handler,
            processor_context,
            basic_settings: &basics.settings,
        });
        if let HitInstructionResponse::CausedEffect(pass_2_control_results) = response {
            // Second pass, without group interaction this time!
            for pass_2_control_result in pass_2_control_results {
                if let Some(pass_2_hi) = pass_2_control_result.hit_instruction {
                    pass_2_hi.execute(HitInstructionContext {
                        mappings: &mut collections.mappings[compartment],
                        control_context,
                        domain_event_handler: &basics.event_handler,
                        processor_context,
                        basic_settings: &basics.settings,
                    });
                }
            }
            if control_result.celebrate_success {
                basics.celebrate_success();
            }
        }
    }
    if let GroupInteractionProcessing::On(input) = group_interaction_processing {
        if input.group_interaction != GroupInteraction::None {
            basics.process_group_interaction(
                collections,
                compartment,
                input.mapping_id,
                input.control_event,
                control_result.at_least_one_target_was_reached,
            );
        }
    }
}

enum ManualFeedbackProcessing<'a> {
    Off,
    On {
        mappings_with_virtual_targets: &'a OrderedMappingMap<MainMapping>,
    },
}

enum GroupInteractionProcessing {
    Off,
    On(GroupInteractionInput),
}

struct ExtendedMappingControlResult {
    control_result: MappingControlResult,
    compartment: CompartmentKind,
    group_interaction_input: GroupInteractionInput,
}

struct GroupInteractionInput {
    mapping_id: MappingId,
    group_interaction: GroupInteraction,
    control_event: ControlEvent<ControlValue>,
}

struct Fb(FeedbackReason, Option<CompoundFeedbackValue>);

impl Fb {
    fn none() -> Self {
        Fb(FeedbackReason::Normal, None)
    }

    fn unused(value: Option<CompoundFeedbackValue>) -> Self {
        Fb(FeedbackReason::ClearUnusedSource, value)
    }

    fn normal(value: Option<CompoundFeedbackValue>) -> Self {
        Fb(FeedbackReason::Normal, value)
    }
}

struct ControlFeedbackSettings {
    control_input: ControlInput,
    feedback_output: Option<FeedbackOutput>,
    control_is_globally_enabled: bool,
    feedback_is_globally_enabled: bool,
}

fn determine_control_globally_enabled(
    context: &ProcessorContext,
    settings: &BasicSettings,
    project_options: ProjectOptions,
) -> bool {
    context.containing_fx().is_enabled()
        && passes_background_project_check(
            context,
            settings.stay_active_when_project_in_background,
            project_options,
        )
}

fn determine_feedback_globally_enabled(
    context: &ProcessorContext,
    settings: &BasicSettings,
    project_options: ProjectOptions,
) -> bool {
    (settings.feedback_output.is_some() || settings.streamdeck_device_id.is_some())
        && context.containing_fx().is_enabled()
        && track_arm_conditions_are_met(context, settings)
        && passes_background_project_check(
            context,
            settings.stay_active_when_project_in_background,
            project_options,
        )
}

pub fn passes_background_project_check(
    context: &ProcessorContext,
    stay_active_when_project_in_background: StayActiveWhenProjectInBackground,
    opts: ProjectOptions,
) -> bool {
    use StayActiveWhenProjectInBackground::*;
    match stay_active_when_project_in_background {
        Never => is_current_project(context),
        OnlyIfBackgroundProjectIsRunning => {
            match (
                opts.run_background_projects,
                opts.run_stopped_background_projects,
            ) {
                (false, _) => is_current_project(context),
                (true, false) => is_current_project(context) || is_playing(context),
                (true, true) => true,
            }
        }
        Always => true,
    }
}

fn is_current_project(context: &ProcessorContext) -> bool {
    if let Some(project) = context.project() {
        project == Reaper::get().current_project()
    } else {
        // When ReaLearn is on the monitoring FX chain, we always control the current project.
        true
    }
}

fn is_playing(context: &ProcessorContext) -> bool {
    context.project_or_current_project().is_playing()
}

fn track_arm_conditions_are_met(context: &ProcessorContext, settings: &BasicSettings) -> bool {
    if !context.containing_fx().is_input_fx() && !settings.send_feedback_only_if_armed {
        return true;
    }
    match context.track() {
        None => true,
        Some(t) => t.is_available() && t.is_armed(false),
    }
}

fn find_active_main_mapping_connected_to_virtual_control_element(
    main_mappings: &OrderedMappingMap<MainMapping>,
    control_element: VirtualControlElement,
) -> Option<&MainMapping> {
    main_mappings.values().find(|m| {
        m.virtual_source_control_element() == Some(control_element)
            && m.feedback_is_effectively_on()
    })
}

fn all_mappings_without_virtual_targets(
    mappings: &EnumMap<CompartmentKind, OrderedMappingMap<MainMapping>>,
) -> impl Iterator<Item = &MainMapping> {
    CompartmentKind::enum_iter().flat_map(move |compartment| mappings[compartment].values())
}

#[derive(Debug, Default)]
struct TargetBasedConditionalActivationProcessor {
    mapping_relations: NonCryptoHashSet<MappingRelation>,
}

#[derive(Eq, PartialEq, Hash, Debug)]
struct MappingRelation {
    lead_mapping: MappingId,
    follow_mapping: MappingId,
}

impl TargetBasedConditionalActivationProcessor {
    pub fn get_follow_mappings(
        &self,
        lead_mapping: MappingId,
    ) -> impl Iterator<Item = MappingId> + '_ {
        self.mapping_relations
            .iter()
            .filter(move |r| r.lead_mapping == lead_mapping)
            .map(|r| r.follow_mapping)
    }

    pub fn clear(&mut self) {
        self.mapping_relations.clear();
    }

    pub fn notify_usage(
        &mut self,
        follow_mapping: MappingId,
        lead_mappings: impl Iterator<Item = MappingId>,
    ) {
        // At first remove all occurrences of follow mapping
        self.mapping_relations
            .retain(|relation| relation.follow_mapping != follow_mapping);
        // Then add
        self.notify_usage_add_only(follow_mapping, lead_mappings);
    }

    pub fn notify_usage_add_only(
        &mut self,
        follow_mapping: MappingId,
        lead_mappings: impl Iterator<Item = MappingId>,
    ) {
        for lead_mapping in lead_mappings {
            let relation = MappingRelation {
                lead_mapping,
                follow_mapping,
            };
            self.mapping_relations.insert(relation);
        }
    }

    pub fn lead_mappings(&self) -> impl Iterator<Item = MappingId> {
        let lead_mapping_id_set: NonCryptoHashSet<_> = self
            .mapping_relations
            .iter()
            .map(|rel| rel.lead_mapping)
            .collect();
        lead_mapping_id_set.into_iter()
    }

    pub fn is_lead_mapping(&self, mapping_id: MappingId) -> bool {
        self.mapping_relations
            .iter()
            .any(|r| r.lead_mapping == mapping_id)
    }
}

fn determine_final_target_value_for_conditional_activation(
    target_is_active: bool,
    target_value: Option<AbsoluteValue>,
) -> Result<Option<AbsoluteValue>, &'static str> {
    if target_is_active {
        if let Some(v) = target_value {
            // Normal case. Target is active. We provide the new value.
            Ok(Some(v))
        } else {
            // Target is active (implies it successfully resolved) but it can't return a
            // current value. We consider this as something temporary and don't do anything.
            Err("target value not available")
        }
    } else {
        // Target is inactive. We let the condition distinguish this case by passing `None`.
        Ok(None)
    }
}

pub struct KeyProcessingResult {
    pub match_outcome: MatchOutcome,
    /// Whether this message should be filtered out from the keyboard processing chain.
    pub filter_out_event: bool,
}

type UnusedSources = NonCryptoHashMap<CompoundMappingSourceAddress, CompoundFeedbackValue>;

struct ProcessVirtualResult {
    control_results: Vec<ExtendedMappingControlResult>,
    match_outcome: MatchOutcome,
}
