use crate::domain::{
    classify_midi_message, match_partially, BasicSettings, CompartmentKind, CompoundMappingSource,
    ControlEvent, ControlEventTimestamp, ControlLogEntry, ControlLogEntryKind, ControlMainTask,
    ControlMode, ControlOptions, FeedbackSendBehavior, LifecycleMidiMessage, LifecyclePhase,
    MappingCore, MappingId, MatchOutcome, MidiClockCalculator, MidiEvent,
    MidiMessageClassification, MidiScanResult, MidiScanner, MidiTransformationContainer,
    NormalRealTimeToMainThreadTask, OrderedMappingMap, OwnedIncomingMidiMessage,
    PersistentMappingProcessingState, QualifiedMappingId, RealTimeCompoundMappingTarget,
    RealTimeControlContext, RealTimeMapping, RealTimeReaperTarget, SampleOffset, UnitId,
    VirtualSourceValue, WeakRealTimeInstance,
};
use helgoboss_learn::{ControlValue, MidiSourceValue, ModeControlResult, RawMidiEvent};
use helgoboss_midi::{
    Channel, ControlChange14BitMessage, ControlChange14BitMessageScanner, DataEntryByteOrder,
    ParameterNumberMessage, PollingParameterNumberMessageScanner, RawShortMessage, ShortMessage,
    ShortMessageFactory, ShortMessageType,
};
use reaper_high::{MidiOutputDevice, Reaper};
use reaper_medium::{
    Hz, MidiInputDeviceId, MidiOutputDeviceId, OnAudioBufferArgs, ProjectRef, SendMidiTime,
};

use base::{NamedChannelSender, SenderToNormalThread, SenderToRealTimeThread};
use enum_map::{enum_map, EnumMap};
use helgobox_allocator::permit_alloc;
use std::convert::TryInto;
use std::ptr::null_mut;
use std::time::Duration;
use tracing::{debug, trace};
use vst::api::{EventType, Events, SysExEvent};
use vst::host::Host;
use vst::plugin::HostCallback;

const NORMAL_BULK_SIZE: usize = 100;
const FEEDBACK_BULK_SIZE: usize = 100;

#[derive(Debug)]
pub struct RealTimeProcessor {
    unit_id: UnitId,
    // Synced processing settings
    settings: BasicSettings,
    control_mode: ControlMode,
    mappings: EnumMap<CompartmentKind, OrderedMappingMap<RealTimeMapping>>,
    // State
    control_is_globally_enabled: bool,
    feedback_is_globally_enabled: bool,
    // Inter-thread communication
    normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
    feedback_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
    normal_main_task_sender: SenderToNormalThread<NormalRealTimeToMainThreadTask>,
    control_main_task_sender: SenderToNormalThread<ControlMainTask>,
    // Scanners for more complex MIDI message types
    nrpn_scanner: PollingParameterNumberMessageScanner,
    cc_14_bit_scanner: ControlChange14BitMessageScanner,
    // For MIDI capturing
    midi_scanner: MidiScanner,
    // For MIDI timing clock calculations
    midi_clock_calculator: MidiClockCalculator,
    sample_rate: Hz,
    instance: WeakRealTimeInstance,
}

impl RealTimeProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        unit_id: UnitId,
        instance: WeakRealTimeInstance,
        normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
        feedback_task_sender: SenderToRealTimeThread<FeedbackRealTimeTask>,
        normal_main_task_sender: SenderToNormalThread<NormalRealTimeToMainThreadTask>,
        control_main_task_sender: SenderToNormalThread<ControlMainTask>,
    ) -> RealTimeProcessor {
        use CompartmentKind::*;
        RealTimeProcessor {
            unit_id,
            instance,
            settings: Default::default(),
            control_mode: ControlMode::Controlling,
            normal_task_receiver,
            feedback_task_receiver,
            feedback_task_sender,
            normal_main_task_sender,
            control_main_task_sender,
            mappings: enum_map! {
                Controller => ordered_map_with_capacity(1000),
                Main => ordered_map_with_capacity(5000),
            },
            nrpn_scanner: PollingParameterNumberMessageScanner::new(Duration::from_millis(1)),
            cc_14_bit_scanner: Default::default(),
            midi_scanner: Default::default(),
            midi_clock_calculator: Default::default(),
            control_is_globally_enabled: false,
            feedback_is_globally_enabled: false,
            sample_rate: Hz::new_panic(1.0),
        }
    }

    pub fn process_incoming_midi_from_vst(
        &mut self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        is_transport_start: bool,
        host: &HostCallback,
    ) {
        if self.settings.midi_control_input() == MidiControlInput::FxInput {
            // TODO-medium Maybe also filter when transport stopping
            if is_transport_start
                && event
                    .payload()
                    .payload()
                    .might_be_automatically_generated_by_reaper()
            {
                // Ignore note off messages which are a result of starting the transport. They
                // are generated by REAPER in order to stop instruments from sounding. But ReaLearn
                // is not an instrument in the classical sense. We don't want to reset target values
                // just because play has been pressed!
                self.process_unmatched(event.payload(), Caller::Vst(host));
                return;
            }
            self.process_incoming_midi(event, Caller::Vst(host), None);
        } else {
            // #33, #290 If MIDI input device is not set to <FX input>, we want to pass through all
            // messages that arrive on FX input.
            self.send_incoming_midi_to_fx_output(event.payload(), Caller::Vst(host))
        }
    }

    pub fn run_from_vst(&mut self, host: &HostCallback) {
        self.process_feedback_tasks(Caller::Vst(host));
    }

    /// This should be regularly called by audio hook in normal mode.
    pub fn run_from_audio_hook_all(
        &mut self,
        might_be_rebirth: bool,
        timestamp: ControlEventTimestamp,
    ) {
        self.run_from_audio_hook_essential(might_be_rebirth);
        self.run_from_audio_hook_control_and_learn(timestamp);
    }

    pub fn midi_control_input(&self) -> MidiControlInput {
        self.settings.midi_control_input()
    }

    pub fn control_is_globally_enabled(&self) -> bool {
        self.control_is_globally_enabled
    }

    /// This should be called by audio hook in normal mode whenever it receives a MIDI message that
    /// is relevant *for this ReaLearn instance* (the input device is not checked again).
    ///
    /// Returns whether this message should be filtered out from the global MIDI stream.
    pub fn process_incoming_midi_from_audio_hook(
        &mut self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        transformation_container: &mut MidiTransformationContainer,
    ) -> bool {
        let match_outcome =
            self.process_incoming_midi(event, Caller::AudioHook, Some(transformation_container));
        let let_through = (match_outcome.matched_or_consumed()
            && self.settings.let_matched_events_through)
            || (!match_outcome.matched_or_consumed() && self.settings.let_unmatched_events_through);
        !let_through
    }

    fn request_full_sync_and_discard_tasks_if_successful(&mut self) {
        if self
            .normal_main_task_sender
            .try_to_send(NormalRealTimeToMainThreadTask::FullResyncToRealTimeProcessorPlease)
        {
            // Requesting a full resync was successful so we can safely discard accumulated tasks.
            let discarded_normal_task_count = self.normal_task_receiver.try_iter().count();
            let discarded_feedback_task_count = self.feedback_task_receiver.try_iter().count();
            permit_alloc(|| {
                debug!(
                    "Successfully requested full sync. Discarded {} normal and {} feedback tasks.",
                    discarded_normal_task_count, discarded_feedback_task_count
                );
            });
        } else {
            permit_alloc(|| {
                debug!(
                    "Small audio device outage detected but probably related to project load so no action taken.",
                );
            });
        }
    }

    /// This should be regularly called by audio hook even during global MIDI source learning.
    ///
    /// The rebirth parameter is `true` if this could be the first audio cycle after an "unplanned"
    /// downtime of the audio device. It could also be just a downtime related to opening the
    /// project itself, which we detect to some degree. See the code that reacts to this parameter.
    pub fn run_from_audio_hook_essential(&mut self, might_be_rebirth: bool) {
        if might_be_rebirth {
            self.request_full_sync_and_discard_tasks_if_successful();
        }
        // Process occasional tasks sent from other thread (probably main thread)
        let normal_task_count = self.normal_task_receiver.len();
        for task in self.normal_task_receiver.try_iter().take(NORMAL_BULK_SIZE) {
            use NormalRealTimeTask::*;
            match task {
                UpdateControlIsGloballyEnabled(is_enabled) => {
                    self.control_is_globally_enabled = is_enabled;
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    // Handle lifecycle MIDI
                    if self.settings.midi_destination().is_some()
                        && is_enabled != self.feedback_is_globally_enabled
                    {
                        self.send_lifecycle_midi_for_all_mappings(is_enabled.into());
                    }
                    // Set
                    self.feedback_is_globally_enabled = is_enabled;
                }
                UpdateAllMappings(compartment, mappings) => {
                    permit_alloc(|| {
                        debug!("Updating {} mappings in {}...", mappings.len(), compartment);
                    });
                    // Handle deactivation MIDI
                    if self.processor_feedback_is_effectively_on() {
                        self.send_lifecycle_midi_for_all_mappings_in(
                            compartment,
                            LifecyclePhase::Deactivation,
                        );
                    }
                    // Clear existing mappings
                    self.mappings[compartment].clear();
                    // Set new mappings
                    self.mappings[compartment].extend(mappings.into_iter().map(|m| (m.id(), m)));
                    // Handle activation MIDI
                    if self.processor_feedback_is_effectively_on() {
                        self.send_lifecycle_midi_for_all_mappings_in(
                            compartment,
                            LifecyclePhase::Activation,
                        );
                    }
                }
                UpdateSingleMapping(compartment, m) => {
                    permit_alloc(|| {
                        debug!("Updating single mapping {:?} in {}...", m.id(), compartment,);
                    });
                    // Send lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        let was_on_before = self.mappings[compartment]
                            .get(&m.id())
                            .is_some_and(|m| m.feedback_is_effectively_on());
                        let is_on_now = m.feedback_is_effectively_on();
                        self.send_lifecycle_midi_diff(&m, was_on_before, is_on_now)
                    }
                    // Update
                    self.mappings[compartment].insert(m.id(), *m);
                }
                UpdatePersistentMappingProcessingState { id, state } => {
                    permit_alloc(|| {
                        debug!(
                            "Updating persistent state of {:?} in {}...",
                            id.id, id.compartment
                        );
                    });
                    // Update
                    let (was_on_before, is_on_now) =
                        if let Some(m) = self.mappings[id.compartment].get_mut(&id.id) {
                            let was_on_before = m.feedback_is_effectively_on();
                            m.update_persistent_processing_state(state);
                            (was_on_before, m.feedback_is_effectively_on())
                        } else {
                            (false, false)
                        };
                    // Send lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        if let Some(m) = self.mappings[id.compartment].get(&id.id) {
                            self.send_lifecycle_midi_diff(m, was_on_before, is_on_now);
                        }
                    }
                }
                UpdateTargetsPartially(compartment, mut target_updates) => {
                    // Apply updates
                    for update in target_updates.iter_mut() {
                        if let Some(m) = self.mappings[compartment].get_mut(&update.id) {
                            m.update_target(update);
                        }
                    }
                    // Handle lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        for update in target_updates.iter() {
                            if let Some(activation_change) = update.activation_change {
                                if let Some(m) = self.mappings[compartment].get(&update.id) {
                                    if m.feedback_is_effectively_on_ignoring_target_activation() {
                                        self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                                            m,
                                            activation_change.is_active.into(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                UpdateSettings(settings) => {
                    permit_alloc(|| {
                        debug!("Updating settings...");
                    });
                    let prev_midi_destination = self.settings.midi_destination();
                    let next_midi_destination = settings.midi_destination();
                    self.settings = settings;
                    let midi_destination_changing = prev_midi_destination != next_midi_destination;
                    // Handle deactivation
                    if self.processor_feedback_is_effectively_on() && midi_destination_changing {
                        self.send_lifecycle_midi_for_all_mappings(LifecyclePhase::Deactivation);
                    }
                    // Handle activation
                    if self.processor_feedback_is_effectively_on() && midi_destination_changing {
                        self.send_lifecycle_midi_for_all_mappings(LifecyclePhase::Activation);
                    }
                }
                UpdateSampleRate(sample_rate) => {
                    permit_alloc(|| {
                        debug!("Updating sample rate");
                    });
                    self.sample_rate = sample_rate;
                }
                StartLearnSource {
                    allow_virtual_sources,
                } => {
                    permit_alloc(|| {
                        debug!("Start learning source");
                    });
                    self.control_mode = ControlMode::LearningSource {
                        allow_virtual_sources,
                        osc_arg_index_hint: None,
                    };
                    self.midi_scanner.reset();
                }
                DisableControl => {
                    permit_alloc(|| {
                        debug!("Disable control");
                    });
                    self.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    permit_alloc(|| {
                        debug!("Return to control mode");
                    });
                    self.control_mode = ControlMode::Controlling;
                    self.nrpn_scanner.reset();
                    self.cc_14_bit_scanner.reset();
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                LogMapping(compartment, mapping_id) => {
                    self.log_mapping(compartment, mapping_id);
                }
                UpdateMappingsPartially(compartment, mapping_updates) => {
                    permit_alloc(|| {
                        debug!("Updating mapping activations...");
                    });
                    // Apply updates
                    for update in mapping_updates.iter() {
                        if let Some(m) = self.mappings[compartment].get_mut(&update.id) {
                            m.update(update);
                        }
                    }
                    // Handle lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        for update in mapping_updates.iter() {
                            if let Some(m) = self.mappings[compartment].get(&update.id) {
                                if let Some(activation_change) = update.activation_change {
                                    if m.feedback_is_effectively_on_ignoring_mapping_activation() {
                                        self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                                            m,
                                            activation_change.is_active.into(),
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

    fn send_lifecycle_midi_diff(&self, m: &RealTimeMapping, was_on_before: bool, is_on_now: bool) {
        if is_on_now {
            self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                m,
                LifecyclePhase::Activation,
            );
        } else if was_on_before {
            self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                m,
                LifecyclePhase::Deactivation,
            );
        }
    }

    fn processor_feedback_is_effectively_on(&self) -> bool {
        self.feedback_is_globally_enabled && self.settings.midi_destination().is_some()
    }

    fn send_lifecycle_midi_for_all_mappings(&self, phase: LifecyclePhase) {
        for compartment in CompartmentKind::enum_iter() {
            self.send_lifecycle_midi_for_all_mappings_in(compartment, phase);
        }
    }

    fn send_lifecycle_midi_for_all_mappings_in(
        &self,
        compartment: CompartmentKind,
        phase: LifecyclePhase,
    ) {
        for m in self.mappings[compartment].values() {
            if m.feedback_is_effectively_on() {
                self.send_lifecycle_midi_to_feedback_output_from_audio_hook(m, phase);
            }
        }
    }

    /// This should *not* be called by the global audio hook when it's globally capturing MIDI
    /// because we want to pause controlling in that case!
    fn run_from_audio_hook_control_and_learn(&mut self, timestamp: ControlEventTimestamp) {
        match self.control_mode {
            ControlMode::Disabled => {}
            ControlMode::Controlling => {
                // This NRPN scanner is just for controlling, not for learning.
                if self.control_is_globally_enabled {
                    // Poll (N)RPN scanner
                    for ch in 0..16 {
                        if let Some(nrpn_msg) = self.nrpn_scanner.poll(Channel::new(ch)) {
                            // TODO-medium We should memorize the offset of the latest short message
                            //  making up the NRPN message instead!
                            let midi_event = MidiEvent::without_offset(nrpn_msg);
                            let control_event = ControlEvent::new(midi_event, timestamp);
                            self.process_incoming_midi_normal_nrpn(
                                control_event,
                                Caller::AudioHook,
                                &mut None,
                            );
                        }
                    }
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                ..
            } => {
                // For local learning/filtering
                if let Some(res) = self.midi_scanner.poll() {
                    self.send_captured_midi(res, allow_virtual_sources);
                }
            }
        }
    }

    fn process_feedback_tasks(&self, caller: Caller) {
        // Process (frequent) feedback tasks sent from other thread (probably main thread)
        for task in self
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_BULK_SIZE)
        {
            use FeedbackRealTimeTask::*;
            match task {
                FxOutputFeedback(v) => {
                    // If the feedback driver is not VST, this will be discarded, no problem.
                    self.send_midi_feedback(v, caller);
                }
                SendLifecycleMidi(compartment, mapping_id, phase) => {
                    if let Some(m) = self.mappings[compartment].get(&mapping_id) {
                        self.send_lifecycle_midi_to_fx_output(
                            m.lifecycle_midi_messages(phase),
                            caller,
                        );
                    }
                }
                NonAllocatingFxOutputFeedback(evt, sample_offset) => {
                    send_raw_midi_to_fx_output(evt.bytes(), sample_offset, caller);
                }
            }
        }
    }

    fn log_debug_info(&self, task_count: usize) {
        // Summary
        permit_alloc(|| {
            let msg = format!(
                "\n\
            # Real-time processor\n\
            \n\
            - Instance ID: {} \n\
            - State: {:?} \n\
            - Total main mapping count: {} \n\
            - Enabled main mapping count: {} \n\
            - Total controller mapping count: {} \n\
            - Enabled controller mapping count: {} \n\
            - Normal task count: {} \n\
            - Feedback task count: {} \n\
            ",
                self.unit_id,
                self.control_mode,
                self.mappings[CompartmentKind::Main].len(),
                self.mappings[CompartmentKind::Main]
                    .values()
                    .filter(|m| m.control_is_effectively_on())
                    .count(),
                self.mappings[CompartmentKind::Controller].len(),
                self.mappings[CompartmentKind::Controller]
                    .values()
                    .filter(|m| m.control_is_effectively_on())
                    .count(),
                task_count,
                self.feedback_task_receiver.len(),
            );
            self.normal_main_task_sender
                .send_complaining(NormalRealTimeToMainThreadTask::LogToConsole(msg));
            // Detailled
            trace!(
                "\n\
            # Real-time processor\n\
            \n\
            {:#?}
            ",
                self
            );
        });
    }

    fn log_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        permit_alloc(|| {
            let mapping = self.mappings[compartment].get(&mapping_id);
            let msg = format!(
                "\n\
            # Real-time processor\n\
            \n\
            Mapping with ID {mapping_id}:\n\
            {mapping:#?}
            "
            );
            self.normal_main_task_sender
                .send_complaining(NormalRealTimeToMainThreadTask::LogToConsole(msg));
        });
    }

    fn process_incoming_midi(
        &mut self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        caller: Caller,
        mut transformation_container: Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        use MidiMessageClassification::*;
        match classify_midi_message(event.payload().payload()) {
            Normal => self.process_incoming_midi_normal(event, caller, transformation_container),
            Ignored => {
                // ReaLearn doesn't process those. Forward them if user wants it.
                self.process_unmatched(event.payload(), caller);
                MatchOutcome::Unmatched
            }
            Timing => {
                // Timing clock messages are treated special (calculates BPM). Matching each tick with
                // mappings would be a waste of resources. We know they come in very densely.
                // This is control-only, we never learn it.
                if self.control_is_globally_enabled {
                    if let Some(bpm) = self.midi_clock_calculator.feed(event.timestamp()) {
                        let source_value = MidiSourceValue::<RawShortMessage>::Tempo(bpm);
                        self.control_midi(
                            event.with_payload(MidiEvent::new(
                                event.payload().offset(),
                                &source_value,
                            )),
                            caller,
                            &mut transformation_container,
                        )
                    } else {
                        MatchOutcome::Unmatched
                    }
                } else {
                    MatchOutcome::Unmatched
                }
            }
        }
    }

    /// This basically splits the stream of short MIDI messages into 3 streams:
    ///
    /// - (N)RPN messages
    /// - 14-bit CC messages
    /// - Short MIDI messaages
    fn process_incoming_midi_normal(
        &mut self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        caller: Caller,
        mut transformation_container: Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        match self.control_mode {
            ControlMode::Controlling => {
                if self.control_is_globally_enabled {
                    // Even if a composite message ((N)RPN or CC 14-bit) was scanned, we still
                    // process the plain short MIDI message. This is desired.
                    // Rationale: If there's no mapping with a composite source
                    // of this kind, then all the CCs potentially involved in
                    // composite messages can still be used separately (e.g. CC
                    // 6, 38, 98, etc.). That's important! However, if there's
                    // at least one mapping source that listens to composite
                    // messages of the incoming kind, we need to make sure that the
                    // single messages can't be used anymore! Otherwise it would be
                    // confusing. They are consumed. That's the reason why
                    // we do the consumption check at a later state.
                    let plain_match_outcome = self.process_incoming_midi_normal_plain(
                        event,
                        caller,
                        &mut transformation_container,
                    );
                    let midi_event = event.payload();
                    let (nrpn_match_outcome, cc14_match_outcome) = match midi_event.payload() {
                        IncomingMidiMessage::Short(short_msg) => {
                            let mut nrpn_match_outcome = MatchOutcome::Unmatched;
                            for nrpn_msg in self.nrpn_scanner.feed(&short_msg).iter().flatten() {
                                let nrpn_event = event
                                    .with_payload(MidiEvent::new(midi_event.offset(), *nrpn_msg));
                                let child_match_outcome = self.process_incoming_midi_normal_nrpn(
                                    nrpn_event,
                                    caller,
                                    &mut transformation_container,
                                );
                                nrpn_match_outcome.upgrade_from(child_match_outcome);
                            }
                            let cc14_match_outcome = if let Some(cc14_msg) =
                                self.cc_14_bit_scanner.feed(&short_msg)
                            {
                                let cc14_event = event
                                    .with_payload(MidiEvent::new(midi_event.offset(), cc14_msg));
                                self.process_incoming_midi_normal_cc14(
                                    cc14_event,
                                    caller,
                                    &mut transformation_container,
                                )
                            } else {
                                MatchOutcome::Unmatched
                            };
                            (nrpn_match_outcome, cc14_match_outcome)
                        }
                        // A sys-ex message is never part of a compound message.
                        IncomingMidiMessage::SysEx(_) => {
                            (MatchOutcome::Unmatched, MatchOutcome::Unmatched)
                        }
                    };
                    plain_match_outcome
                        .merge_with(nrpn_match_outcome)
                        .merge_with(cc14_match_outcome)
                } else {
                    MatchOutcome::Unmatched
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                ..
            } => {
                if self.settings.real_input_logging_enabled {
                    self.log_real_learn_input(event.map_payload(|e| e.payload()));
                }
                let scan_result = match event.payload().payload() {
                    IncomingMidiMessage::Short(short_msg) => {
                        self.midi_scanner.feed_short(short_msg, None)
                    }
                    IncomingMidiMessage::SysEx(bytes) => {
                        // It's okay here to temporarily permit allocation because crackling during
                        // learning is not a showstopper.
                        permit_alloc(|| MidiScanResult::try_from_bytes(bytes, None).ok())
                    }
                };
                if let Some(source) = scan_result {
                    self.send_captured_midi(source, allow_virtual_sources);
                }
                MatchOutcome::Consumed
            }
            ControlMode::Disabled => {
                // "Disabled" means we use this for global learning! We consider this therefore as
                // consumed.
                MatchOutcome::Consumed
            }
        }
    }

    /// Returns whether this message matched.
    fn process_incoming_midi_normal_nrpn(
        &mut self,
        event: ControlEvent<MidiEvent<ParameterNumberMessage>>,
        caller: Caller,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        let midi_event = event.payload();
        let source_value =
            MidiSourceValue::<RawShortMessage>::ParameterNumber(midi_event.payload());
        let match_outcome = self.control_midi(
            event.with_payload(MidiEvent::new(midi_event.offset(), &source_value)),
            caller,
            transformation_container,
        );
        if self.settings.real_input_logging_enabled {
            self.log_real_control_input_internal(event.with_payload(source_value), match_outcome);
        }
        if self.settings.midi_control_input() == MidiControlInput::FxInput
            && ((match_outcome.matched_or_consumed() && self.settings.let_matched_events_through)
                || (!match_outcome.matched_or_consumed()
                    && self.settings.let_unmatched_events_through))
        {
            for m in event
                .payload()
                .payload()
                .to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst)
                .iter()
                .flatten()
            {
                self.send_short_midi_to_fx_output(
                    MidiEvent::new(event.payload().offset(), *m),
                    caller,
                );
            }
        }
        match_outcome
    }

    /// Might allocate!
    fn log_real_control_input(
        &self,
        evt: ControlEvent<MidiSourceValue<RawShortMessage>>,
        consumed: bool,
        matched: bool,
    ) {
        let match_outcome = if consumed {
            MatchOutcome::Consumed
        } else if matched {
            MatchOutcome::Matched
        } else {
            MatchOutcome::Unmatched
        };
        self.log_real_control_input_internal(evt, match_outcome)
    }

    fn log_real_control_input_internal(
        &self,
        evt: ControlEvent<MidiSourceValue<RawShortMessage>>,
        match_outcome: MatchOutcome,
    ) {
        // It's okay to crackle when logging input.
        let timestamp = evt.timestamp();
        let owned_msg = permit_alloc(|| evt.into_payload().try_into_owned());
        if let Ok(msg) = owned_msg {
            self.control_main_task_sender
                .send_complaining(ControlMainTask::LogRealControlInput {
                    event: ControlEvent::new(msg, timestamp),
                    match_outcome,
                });
        }
    }

    /// Might allocate!
    fn log_real_learn_input(&self, evt: ControlEvent<IncomingMidiMessage>) {
        // It's okay if we crackle when logging input.
        let owned_msg = permit_alloc(|| evt.payload().to_owned());
        self.control_main_task_sender
            .send_complaining(ControlMainTask::LogRealLearnInput {
                event: evt.with_payload(owned_msg),
            });
    }

    /// Might allocate!
    fn log_lifecycle_output(&self, value: MidiSourceValue<RawShortMessage>) {
        // It's okay to crackle when logging input.
        if let Ok(value) = permit_alloc(|| value.try_into_owned()) {
            self.normal_main_task_sender
                .send_complaining(NormalRealTimeToMainThreadTask::LogLifecycleOutput { value });
        }
    }

    fn send_captured_midi(&mut self, scan_result: MidiScanResult, allow_virtual_sources: bool) {
        // If plug-in dropped, the receiver might be gone already because main processor is
        // unregistered synchronously.
        self.normal_main_task_sender
            .send_if_space(NormalRealTimeToMainThreadTask::CaptureMidi {
                scan_result,
                allow_virtual_sources,
            });
    }

    /// Returns whether this message matched.
    fn process_incoming_midi_normal_cc14(
        &mut self,
        event: ControlEvent<MidiEvent<ControlChange14BitMessage>>,
        caller: Caller,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        let midi_event = event.payload();
        let source_value =
            MidiSourceValue::<RawShortMessage>::ControlChange14Bit(midi_event.payload());
        let match_outcome = self.control_midi(
            event.with_payload(MidiEvent::new(midi_event.offset(), &source_value)),
            caller,
            transformation_container,
        );
        if self.settings.real_input_logging_enabled {
            self.log_real_control_input_internal(event.with_payload(source_value), match_outcome);
        }
        if self.settings.midi_control_input() == MidiControlInput::FxInput
            && ((match_outcome.matched_or_consumed() && self.settings.let_matched_events_through)
                || (!match_outcome.matched_or_consumed()
                    && self.settings.let_unmatched_events_through))
        {
            for m in midi_event
                .payload()
                .to_short_messages::<RawShortMessage>()
                .iter()
            {
                let short_event = MidiEvent::new(midi_event.offset(), *m);
                self.send_short_midi_to_fx_output(short_event, caller);
            }
        }
        match_outcome
    }

    fn process_incoming_midi_normal_plain(
        &mut self,
        event: ControlEvent<MidiEvent<IncomingMidiMessage>>,
        caller: Caller,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        let midi_event = event.payload();
        let source_value = midi_event.payload().to_source_value();
        if self.is_consumed_by_at_least_one_source(midi_event.payload()) {
            if self.settings.real_input_logging_enabled {
                self.log_real_control_input(event.with_payload(source_value), true, false);
            }
            // Some short MIDI messages are just parts of bigger composite MIDI messages,
            // e.g. (N)RPN or 14-bit CCs. If we reach this point, the incoming message
            // could potentially match one of the (N)RPN or 14-bit CC mappings in the list
            // and therefore doesn't qualify anymore as a candidate for normal CC sources.
            return MatchOutcome::Consumed;
        }
        let match_outcome = self.control_midi(
            event.with_payload(MidiEvent::new(midi_event.offset(), &source_value)),
            caller,
            transformation_container,
        );
        if self.settings.real_input_logging_enabled {
            self.log_real_control_input_internal(event.with_payload(source_value), match_outcome);
        }
        // At this point, we shouldn't have "consumed" anymore because for MIDI sources, no
        // control will be done at all if a message is consumed by at least one mapping (see above).
        if match_outcome.matched_or_consumed() {
            self.process_matched_short(midi_event, caller);
        } else {
            self.process_unmatched(midi_event, caller);
        }
        match_outcome
    }

    fn all_mappings(&self) -> impl Iterator<Item = &RealTimeMapping> {
        CompartmentKind::enum_iter()
            .flat_map(move |compartment| self.mappings[compartment].values())
    }

    fn control_midi(
        &mut self,
        value_event: ControlEvent<MidiEvent<&MidiSourceValue<RawShortMessage>>>,
        caller: Caller,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        let is_rendering = is_rendering();
        // We do pattern matching in order to use Rust's borrow splitting.
        let controller_outcome = if let [ref mut controller_mappings, ref mut main_mappings] =
            self.mappings.as_mut_slice()
        {
            control_controller_mappings_midi(
                &self.settings,
                &self.control_main_task_sender,
                &self.feedback_task_sender,
                controller_mappings,
                main_mappings,
                value_event,
                caller,
                &self.instance,
                is_rendering,
                transformation_container,
            )
        } else {
            unreachable!()
        };
        let main_outcome = self.control_main_mappings_midi(
            value_event,
            caller,
            is_rendering,
            transformation_container,
        );
        controller_outcome.merge_with(main_outcome)
    }

    fn control_main_mappings_midi(
        &mut self,
        source_value_event: ControlEvent<MidiEvent<&MidiSourceValue<RawShortMessage>>>,
        caller: Caller,
        is_rendering: bool,
        transformation_container: &mut Option<&mut MidiTransformationContainer>,
    ) -> MatchOutcome {
        let match_inactive = self.settings.match_even_inactive_mappings;
        let compartment = CompartmentKind::Main;
        let mut match_outcome = MatchOutcome::Unmatched;
        for m in self.mappings[compartment]
            .values_mut()
            // Consider only control-enabled real mappings.
            // The UI prevents creating main mappings with virtual targets but a JSON import
            // doesn't. Check again that it's a REAPER target.
            .filter(|m| m.core.options.control_is_enabled && m.has_reaper_target())
        {
            let matched_already_before = match_outcome.matched();
            let mapping_is_active = m.is_active();
            if !mapping_is_active && !match_inactive {
                continue;
            }
            let CompoundMappingSource::Midi(s) = &m.source() else {
                continue;
            };
            let midi_event = source_value_event.payload();
            let Some(control_value) = s.control(midi_event.payload()) else {
                continue;
            };
            // It can't be consumed because we checked this before for all mappings.
            if mapping_is_active {
                let args = ProcessRtMappingArgs {
                    main_task_sender: &self.control_main_task_sender,
                    rt_feedback_sender: &self.feedback_task_sender,
                    compartment,
                    value_event: source_value_event
                        .with_payload(MidiEvent::new(midi_event.offset(), control_value)),
                    options: ControlOptions {
                        enforce_send_feedback_after_control: false,
                        mode_control_options: Default::default(),
                        enforce_target_refresh: matched_already_before,
                        coming_from_real_time: true,
                    },
                    caller,
                    midi_feedback_output: self.settings.midi_destination(),
                    log_options: LogOptions::from_basic_settings(&self.settings),
                    instance: &self.instance,
                    is_rendering,
                    transformation_container,
                };
                process_real_mapping(m, args);
            }
            match_outcome = MatchOutcome::Matched;
        }
        match_outcome
    }

    fn process_matched_short(&self, event: MidiEvent<IncomingMidiMessage>, caller: Caller) {
        if self.settings.midi_control_input() != MidiControlInput::FxInput {
            return;
        }
        if !self.settings.let_matched_events_through {
            return;
        }
        self.send_incoming_midi_to_fx_output(event, caller);
    }

    fn process_unmatched(&self, event: MidiEvent<IncomingMidiMessage>, caller: Caller) {
        if self.settings.midi_control_input() != MidiControlInput::FxInput {
            return;
        }
        if !self.settings.let_unmatched_events_through {
            return;
        }
        self.send_incoming_midi_to_fx_output(event, caller);
    }

    fn is_consumed_by_at_least_one_source(&self, msg: IncomingMidiMessage) -> bool {
        use IncomingMidiMessage::*;
        match msg {
            Short(msg) => self
                .all_mappings()
                .any(|m| m.control_is_effectively_on() && m.consumes(msg)),
            // Sys-ex is never part of a compound message.
            SysEx(_) => false,
        }
    }

    fn send_midi_feedback(&self, value: MidiSourceValue<RawShortMessage>, caller: Caller) {
        if let Some(evts) = value.to_raw() {
            // TODO-medium We can implement in a way so we only need one host.process_events() call.
            for evt in evts {
                send_raw_midi_to_fx_output(evt.bytes(), SampleOffset::ZERO, caller);
            }
        } else {
            let shorts = value.to_short_messages(DataEntryByteOrder::MsbFirst);
            if shorts[0].is_none() {
                return;
            }
            for short in shorts.iter().flatten() {
                self.send_short_midi_to_fx_output(MidiEvent::without_offset(*short), caller);
            }
        }
    }

    fn send_lifecycle_midi_to_feedback_output_from_audio_hook(
        &self,
        m: &RealTimeMapping,
        phase: LifecyclePhase,
    ) {
        if let Some(output) = self.settings.midi_destination() {
            match output {
                MidiDestination::FxOutput => {
                    // We can't send it now because we don't have safe access to the host callback
                    // because this method is being called from the audio hook.
                    self.feedback_task_sender.send_if_space(
                        FeedbackRealTimeTask::SendLifecycleMidi(m.compartment(), m.id(), phase),
                    );
                }
                MidiDestination::Device(dev_id) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            for m in m.lifecycle_midi_messages(phase) {
                                match m {
                                    LifecycleMidiMessage::Short(msg) => {
                                        if self.settings.real_output_logging_enabled {
                                            self.log_lifecycle_output(MidiSourceValue::Plain(*msg));
                                        }
                                        mo.send(*msg, SendMidiTime::Instantly);
                                    }
                                    LifecycleMidiMessage::Raw(data) => {
                                        if self.settings.real_output_logging_enabled {
                                            permit_alloc(|| {
                                                // We don't use this as feedback value,
                                                // at least not in the sense that it
                                                // participates in feedback relay.
                                                let feedback_address_info = None;
                                                let value = MidiSourceValue::single_raw(
                                                    feedback_address_info,
                                                    *data.clone(),
                                                );
                                                self.log_lifecycle_output(value);
                                            });
                                        }
                                        mo.send_msg(**data, SendMidiTime::Instantly);
                                    }
                                }
                            }
                        }
                    });
                }
            };
        }
    }

    fn send_lifecycle_midi_to_fx_output(&self, messages: &[LifecycleMidiMessage], caller: Caller) {
        for m in messages {
            match m {
                LifecycleMidiMessage::Short(msg) => {
                    if self.settings.real_output_logging_enabled {
                        self.log_lifecycle_output(MidiSourceValue::Plain(*msg));
                    }
                    self.send_short_midi_to_fx_output(MidiEvent::without_offset(*msg), caller)
                }
                LifecycleMidiMessage::Raw(data) => {
                    if self.settings.real_output_logging_enabled {
                        permit_alloc(|| {
                            // We don't use this as feedback value,
                            // at least not in the sense that it
                            // participates in feedback relay.
                            let feedback_address_info = None;
                            let value =
                                MidiSourceValue::single_raw(feedback_address_info, *data.clone());
                            self.log_lifecycle_output(value);
                        });
                    }
                    send_raw_midi_to_fx_output(data.bytes(), SampleOffset::ZERO, caller)
                }
            }
        }
    }

    fn send_incoming_midi_to_fx_output(
        &self,
        event: MidiEvent<IncomingMidiMessage>,
        caller: Caller,
    ) {
        match event.payload() {
            IncomingMidiMessage::Short(s) => {
                self.send_short_midi_to_fx_output(MidiEvent::new(event.offset(), s), caller);
            }
            IncomingMidiMessage::SysEx(s) => send_raw_midi_to_fx_output(s, event.offset(), caller),
        }
    }

    fn send_short_midi_to_fx_output(&self, event: MidiEvent<RawShortMessage>, caller: Caller) {
        let host = match caller {
            Caller::Vst(h) => h,
            _ => {
                // We must not forward MIDI to VST output if this was called from the global audio
                // hook. First, it could lead to strange effects because
                // `HostCallback::process_events()` is supposed to be called only
                // from the VST processing method. Second, it could even lead to a
                // crash because the real-time processor is removed from
                // the audio hook *after* the plug-in has been already unregistered, and then
                // invoking the host callback (in particular dereferencing the
                // AEffect) would be illegal. This is just a last safety check.
                // Processing should stop before even calling this method.
                return;
            }
        };
        let vst_event = build_short_midi_vst_event(event);
        let vst_events = build_vst_events(&vst_event as *const _ as _);
        host.process_events(&vst_events);
    }
}

fn build_vst_events(event: *mut vst::api::Event) -> Events {
    Events {
        num_events: 1,
        _reserved: 0,
        events: [event, null_mut()],
    }
}

fn build_sysex_midi_vst_event(bytes: &[u8], offset: SampleOffset) -> SysExEvent {
    SysExEvent {
        event_type: EventType::SysEx,
        byte_size: std::mem::size_of::<SysExEvent>() as _,
        delta_frames: offset.get() as _,
        _flags: 0,
        data_size: bytes.len() as _,
        _reserved1: 0,
        system_data: bytes.as_ptr() as _,
        _reserved2: 0,
    }
}

fn build_short_midi_vst_event(event: MidiEvent<RawShortMessage>) -> vst::api::MidiEvent {
    let bytes = event.payload().to_bytes();
    vst::api::MidiEvent {
        event_type: EventType::Midi,
        byte_size: std::mem::size_of::<vst::api::MidiEvent>() as _,
        delta_frames: event.offset().get() as _,
        flags: vst::api::MidiEventFlags::REALTIME_EVENT.bits(),
        note_length: 0,
        note_offset: 0,
        midi_data: [bytes.0, bytes.1.get(), bytes.2.get()],
        _midi_reserved: 0,
        detune: 0,
        note_off_velocity: 0,
        _reserved1: 0,
        _reserved2: 0,
    }
}

#[derive(Copy, Clone)]
pub enum Caller<'a> {
    Vst(&'a HostCallback),
    AudioHook,
}

impl Caller<'_> {
    pub fn is_vst(&self) -> bool {
        matches!(self, Self::Vst(_))
    }
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalRealTimeTask {
    UpdateAllMappings(CompartmentKind, Vec<RealTimeMapping>),
    UpdateSingleMapping(CompartmentKind, Box<RealTimeMapping>),
    UpdatePersistentMappingProcessingState {
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    },
    UpdateSettings(BasicSettings),
    /// This takes care of propagating target activation states and/or real-time target updates
    /// (for non-virtual mappings).
    UpdateTargetsPartially(CompartmentKind, Vec<RealTimeTargetUpdate>),
    /// Updates the activation state of multiple mappings.
    ///
    /// The given vector contains updates just for affected mappings. This is because when a
    /// parameter update occurs we can determine in a very granular way which targets are affected.
    UpdateMappingsPartially(CompartmentKind, Vec<RealTimeMappingUpdate>),
    LogDebugInfo,
    LogMapping(CompartmentKind, MappingId),
    UpdateSampleRate(Hz),
    StartLearnSource {
        allow_virtual_sources: bool,
    },
    DisableControl,
    ReturnToControlMode,
    UpdateControlIsGloballyEnabled(bool),
    UpdateFeedbackIsGloballyEnabled(bool),
}

#[derive(Copy, Clone, Debug)]
pub struct MappingActivationEffect {
    pub id: MappingId,
    pub active_1_effect: Option<bool>,
    pub active_2_effect: Option<bool>,
}

impl MappingActivationEffect {
    pub fn new(
        id: MappingId,
        active_1_effect: Option<bool>,
        active_2_effect: Option<bool>,
    ) -> Option<MappingActivationEffect> {
        if active_1_effect.is_none() && active_2_effect.is_none() {
            return None;
        }
        let and = MappingActivationEffect {
            id,
            active_1_effect,
            active_2_effect,
        };
        Some(and)
    }
}

/// Depending on the context this can be about mapping activation or target activation.
///
/// It's important that this reflects an actual change, otherwise the real-time processor might
/// send lifecycle MIDI data in the wrong situations.
#[derive(Copy, Clone, Debug)]
pub struct ActivationChange {
    pub is_active: bool,
}

#[derive(Debug)]
pub struct RealTimeTargetUpdate {
    pub id: MappingId,
    pub activation_change: Option<ActivationChange>,
    pub target_change: Option<Option<RealTimeCompoundMappingTarget>>,
}

#[derive(Debug)]
pub struct RealTimeMappingUpdate {
    pub id: MappingId,
    pub activation_change: Option<ActivationChange>,
}

/// A feedback task (which is potentially sent very frequently).
#[derive(Debug)]
// TODO-high-playtime-refactoring Might want to fix this.
#[allow(clippy::large_enum_variant)]
pub enum FeedbackRealTimeTask {
    /// When it comes to MIDI feedback, the real-time processor is only responsible for FX output
    /// feedback. Direct-device feedback is taken care of by the global audio hook for reasons of
    /// proper ordering.
    FxOutputFeedback(MidiSourceValue<'static, RawShortMessage>),
    /// If we send raw MIDI events from the "MIDI: Send message" target to "FX output" and the input
    /// is a MIDI device (not FX input), we must very shortly defer sending the message.
    /// Reason: This message arrives from the audio hook. However, we can't forward to FX output
    /// from the audio hook, we must wait until the VST process method is invoked. In order to let
    /// the MIDI event survive, we need to copy it. But we are not allowed to allocate, so the
    /// usual MidiSourceValue Raw variant is not suited.
    NonAllocatingFxOutputFeedback(RawMidiEvent, SampleOffset),
    /// Used only if feedback output is <FX output>, otherwise done synchronously.
    SendLifecycleMidi(CompartmentKind, MappingId, LifecyclePhase),
}

impl Drop for RealTimeProcessor {
    fn drop(&mut self) {
        permit_alloc(|| {
            debug!("Dropping real-time processor...");
        });
    }
}

/// MIDI source which provides ReaLearn control data.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MidiControlInput {
    /// Processes MIDI messages which are fed into ReaLearn FX.
    FxInput,
    /// Processes MIDI messages coming directly from a MIDI input device.
    Device(MidiInputDeviceId),
}

/// MIDI destination to which e.g. ReaLearn's feedback data can be sent.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MidiDestination {
    /// Routes messages to the ReaLearn FX output.
    FxOutput,
    /// Routes messages directly to a MIDI output device.
    Device(MidiOutputDeviceId),
}

#[allow(clippy::too_many_arguments)]
fn control_controller_mappings_midi(
    settings: &BasicSettings,
    main_task_sender: &SenderToNormalThread<ControlMainTask>,
    rt_feedback_sender: &SenderToRealTimeThread<FeedbackRealTimeTask>,
    // Mappings with virtual targets
    controller_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    // Mappings with virtual sources
    main_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    value_event: ControlEvent<MidiEvent<&MidiSourceValue<RawShortMessage>>>,
    caller: Caller,
    instance: &WeakRealTimeInstance,
    is_rendering: bool,
    transformation_container: &mut Option<&mut MidiTransformationContainer>,
) -> MatchOutcome {
    let match_inactive = settings.match_even_inactive_mappings;
    let midi_feedback_output = settings.midi_destination();
    let log_options = LogOptions::from_basic_settings(settings);
    let mut match_outcome = MatchOutcome::Unmatched;
    let mut enforce_target_refresh = false;
    let evt = flatten_control_midi_event(value_event);
    for m in controller_mappings
        .values_mut()
        .filter(|m| m.core.options.control_is_enabled)
    {
        let mapping_is_active = m.is_active();
        if !mapping_is_active && !match_inactive {
            continue;
        }
        let virtual_target =
            if let Some(RealTimeCompoundMappingTarget::Virtual(t)) = m.resolved_target.as_ref() {
                Some(t)
            } else {
                None
            };
        if virtual_target.is_some() && !mapping_is_active {
            // For mappings with virtual targets, the setting "Match even inactive mappings" isn't relevant.
            // If such mappings are inactive, they will never be considered as matched. Only associated main mappings
            // decide about the match result.
            continue;
        }
        let CompoundMappingSource::Midi(s) = &m.core.source else {
            continue;
        };
        let Some(control_value) = s.control(evt.payload()) else {
            continue;
        };
        if let Some(virtual_target) = virtual_target {
            // Virtual control
            let Some(virtual_source_value) =
                match_partially(&mut m.core, virtual_target, evt.with_payload(control_value))
            else {
                continue;
            };
            let virtual_match_outcome = control_main_mappings_virtual(
                settings,
                main_task_sender,
                rt_feedback_sender,
                main_mappings,
                value_event.with_payload(MidiEvent::new(
                    value_event.payload().offset(),
                    virtual_source_value,
                )),
                ControlOptions {
                    // We inherit "Send feedback after control" to the main processor if it's
                    // enabled for the virtual mapping. That's the easy way to do it.
                    // Downside: If multiple real control elements are mapped to one virtual
                    // control element, "feedback after control" will be sent to all of those,
                    // which is technically not necessary. It would be enough to just send it
                    // to the one that was touched. However, it also doesn't really hurt.
                    enforce_send_feedback_after_control: m.options().feedback_send_behavior
                        == FeedbackSendBehavior::SendFeedbackAfterControl,
                    mode_control_options: m.mode_control_options(),
                    // Not important yet at this point because virtual targets can't affect
                    // subsequent virtual targets.
                    enforce_target_refresh: false,
                    coming_from_real_time: true,
                },
                caller,
                instance,
                is_rendering,
                transformation_container,
            );
            if log_options.virtual_input_logging_enabled {
                log_virtual_control_input(
                    main_task_sender,
                    value_event.with_payload(virtual_source_value),
                    virtual_match_outcome,
                );
            }
            match_outcome.upgrade_from(virtual_match_outcome);
        } else {
            // Real control
            match_outcome.upgrade_from(MatchOutcome::Matched);
            if !mapping_is_active {
                continue;
            }
            if !m.target_is_resolved {
                continue;
            }
            let args = ProcessRtMappingArgs {
                main_task_sender,
                rt_feedback_sender,
                compartment: CompartmentKind::Controller,
                value_event: value_event.with_payload(MidiEvent::new(
                    value_event.payload().offset(),
                    control_value,
                )),
                options: ControlOptions {
                    enforce_send_feedback_after_control: false,
                    mode_control_options: Default::default(),
                    enforce_target_refresh,
                    coming_from_real_time: true,
                },
                caller,
                midi_feedback_output,
                log_options,
                instance,
                is_rendering,
                transformation_container,
            };
            process_real_mapping(m, args);
            // We do this only for transactions of *real* target matches.
            enforce_target_refresh = true;
        }
    }
    match_outcome
}

struct ProcessRtMappingArgs<'a, 'b> {
    main_task_sender: &'a SenderToNormalThread<ControlMainTask>,
    rt_feedback_sender: &'a SenderToRealTimeThread<FeedbackRealTimeTask>,
    compartment: CompartmentKind,
    value_event: ControlEvent<MidiEvent<ControlValue>>,
    options: ControlOptions,
    caller: Caller<'a>,
    midi_feedback_output: Option<MidiDestination>,
    log_options: LogOptions,
    instance: &'a WeakRealTimeInstance,
    is_rendering: bool,
    transformation_container: &'a mut Option<&'b mut MidiTransformationContainer>,
}

#[allow(clippy::too_many_arguments)]
fn process_real_mapping(mapping: &mut RealTimeMapping, mut args: ProcessRtMappingArgs) {
    let pure_control_event = flatten_control_midi_event(args.value_event);
    // At first check if this target is capable of real-time control (= not taking a detour into the main thread).
    if let Some(RealTimeCompoundMappingTarget::Reaper(reaper_target)) =
        mapping.resolved_target.as_mut()
    {
        // We have a real-time-capable target
        if reaper_target.wants_real_time_control(args.caller, args.is_rendering) {
            let forward_to_main_thread = process_real_mapping_in_real_time(
                &mut mapping.core,
                &mut args,
                pure_control_event,
                reaper_target,
            );
            if !forward_to_main_thread {
                // We are done here.
                return;
            }
        }
    }
    // Looks like forwarding to main thread is necessary, e.g. because target is either not a real-time target (in most
    // cases) or doesn't want real-time control at the moment or detected that some things can only be done in the main
    // thread.
    if args.is_rendering {
        return;
    }
    forward_control_to_main_processor(
        args.main_task_sender,
        args.compartment,
        mapping.id(),
        pure_control_event,
        args.options,
    );
}

/// This returns `true` if the consumer should also forward this event to the main thread (used for example for
/// Playtime's trigger-slot action in order to possibly record when slot empty and/or activate the triggered slot).
fn process_real_mapping_in_real_time(
    mapping_core: &mut MappingCore,
    args: &mut ProcessRtMappingArgs,
    pure_control_event: ControlEvent<ControlValue>,
    reaper_target: &mut RealTimeReaperTarget,
) -> bool {
    // REAPER real-time target actually wants real-time control. Try to process directly here in real-time.
    let control_context = RealTimeControlContext {
        instance: args.instance,
        _p: &(),
    };
    let mode_control_result = mapping_core.mode.control_with_options(
        pure_control_event,
        reaper_target,
        control_context,
        args.options.mode_control_options,
        // Performance control not supported when controlling real-time
        None,
    );
    let (log_entry_kind, control_value, error) = match mode_control_result {
        None => (ControlLogEntryKind::IgnoredByGlue, None, ""),
        Some(ModeControlResult::LeaveTargetUntouched(v)) => {
            (ControlLogEntryKind::LeftTargetUntouched, Some(v), "")
        }
        Some(ModeControlResult::HitTarget {
            value: control_value,
        }) => {
            let hit_result = match reaper_target {
                RealTimeReaperTarget::SendMidi(t) => t.midi_send_target_send_midi_in_rt_thread(
                    args.caller,
                    control_value,
                    args.midi_feedback_output,
                    args.log_options,
                    args.main_task_sender,
                    args.rt_feedback_sender,
                    args.value_event.payload(),
                    args.transformation_container,
                ),
                RealTimeReaperTarget::PlaytimeSlotTransport(t) => {
                    let result = t.hit(control_value, control_context);
                    if result.is_ok_and(|forward_to_main_thread| forward_to_main_thread) {
                        // Important: We must forward to main thread in this case in order to possibly record!
                        return true;
                    }
                    result.map(|_| ())
                }
                RealTimeReaperTarget::PlaytimeColumn(t) => t.hit(control_value, control_context),
                RealTimeReaperTarget::PlaytimeRow(t) => t.hit(control_value, control_context),
                RealTimeReaperTarget::PlaytimeMatrix(t) => {
                    let result = t.hit(control_value, control_context);
                    if result.is_ok_and(|forward_to_main_thread| forward_to_main_thread) {
                        // Important: We must forward to main thread in this case!
                        return true;
                    }
                    result.map(|_| ())
                }
                RealTimeReaperTarget::FxParameter(t) => t.hit(control_value),
            };
            match hit_result {
                Ok(_) => (
                    ControlLogEntryKind::HitSuccessfully,
                    Some(control_value),
                    "",
                ),
                Err(e) => (ControlLogEntryKind::HitFailed, Some(control_value), e),
            }
        }
    };
    mapping_core.increase_invocation_count();
    if args.log_options.target_control_logging_enabled {
        let entry = ControlLogEntry {
            kind: log_entry_kind,
            control_value,
            target_index: 0,
            invocation_count: mapping_core.invocation_count(),
            error,
        };
        args.main_task_sender
            .send_complaining(ControlMainTask::LogTargetControl {
                mapping_id: QualifiedMappingId::new(args.compartment, mapping_core.id),
                entry,
            });
    }
    // This means: Everything done. Don't forward event to main thread.
    false
}

fn forward_control_to_main_processor(
    sender: &SenderToNormalThread<ControlMainTask>,
    compartment: CompartmentKind,
    mapping_id: MappingId,
    control_event: ControlEvent<ControlValue>,
    options: ControlOptions,
) {
    let task = ControlMainTask::ControlFromRealTime {
        compartment,
        mapping_id,
        event: control_event,
        options,
    };
    // If plug-in dropped, the receiver might be gone already because main processor is
    // unregistered synchronously.
    sender.send_if_space(task);
}

#[allow(clippy::too_many_arguments)]
fn control_main_mappings_virtual(
    settings: &BasicSettings,
    main_task_sender: &SenderToNormalThread<ControlMainTask>,
    rt_feedback_sender: &SenderToRealTimeThread<FeedbackRealTimeTask>,
    main_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    value_event: ControlEvent<MidiEvent<VirtualSourceValue>>,
    options: ControlOptions,
    caller: Caller,
    instance: &WeakRealTimeInstance,
    is_rendering: bool,
    transformation_container: &mut Option<&mut MidiTransformationContainer>,
) -> MatchOutcome {
    let midi_feedback_output = settings.midi_destination();
    let log_options = LogOptions::from_basic_settings(settings);
    // Controller mappings can't have virtual sources, so for now we only need to check
    // main mappings.
    let match_inactive = settings.match_even_inactive_mappings;
    let mut match_outcome = MatchOutcome::Unmatched;
    let mut controlled_at_least_one = false;
    for m in main_mappings
        .values_mut()
        // Consider only control-enabled main mappings
        .filter(|m| m.core.options.control_is_enabled)
    {
        let mapping_is_active = m.is_active();
        if !mapping_is_active && !match_inactive {
            continue;
        }
        let CompoundMappingSource::Virtual(s) = &m.source() else {
            continue;
        };
        let midi_event = value_event.payload();
        let Some(control_value) = s.control(&midi_event.payload()) else {
            continue;
        };
        // We found an associated main mapping, so it's not just consumed, it's matched.
        match_outcome = MatchOutcome::Matched;
        if !mapping_is_active {
            continue;
        }
        let args = ProcessRtMappingArgs {
            main_task_sender,
            rt_feedback_sender,
            compartment: CompartmentKind::Main,
            value_event: value_event
                .with_payload(MidiEvent::new(midi_event.offset(), control_value)),
            options: ControlOptions {
                enforce_target_refresh: controlled_at_least_one,
                ..options
            },
            caller,
            midi_feedback_output,
            log_options,
            instance,
            is_rendering,
            transformation_container,
        };
        process_real_mapping(m, args);
        controlled_at_least_one = true;
    }
    match_outcome
}

pub fn send_raw_midi_to_fx_output(bytes: &[u8], offset: SampleOffset, caller: Caller) {
    let host = match caller {
        Caller::Vst(h) => h,
        _ => return,
    };
    let event = build_sysex_midi_vst_event(bytes, offset);
    let events = build_vst_events(&event as *const _ as _);
    host.process_events(&events);
}

fn ordered_map_with_capacity<T>(cap: usize) -> OrderedMappingMap<T> {
    let mut map = OrderedMappingMap::with_capacity_and_hasher(cap, Default::default());
    // This is a workaround for an indexmap bug which allocates space for entries on the
    // first extend/reserve call although it should have been done already when creating
    // it via with_capacity. Remember: We must not allocate in real-time thread!
    map.reserve(0);
    map
}

#[derive(Copy, Clone)]
pub enum IncomingMidiMessage<'a> {
    Short(RawShortMessage),
    SysEx(&'a [u8]),
}

impl<'a> MidiEvent<IncomingMidiMessage<'a>> {
    pub fn from_vst(e: vst::event::Event<'a>) -> Result<Self, &'static str> {
        let msg = IncomingMidiMessage::from_vst(e)?;
        let delta_frames = match e {
            vst::event::Event::Midi(e) => e.delta_frames,
            vst::event::Event::SysEx(e) => e.delta_frames,
            vst::event::Event::Deprecated(e) => e.delta_frames,
        };
        // Negative offset was observed in the wild, see
        // https://github.com/helgoboss/helgobox/issues/54. Don't know what that's
        // supposed to mean but falling back to zero should be okay in our case.
        let offset = SampleOffset::new(std::cmp::max(0, delta_frames) as u64);
        Ok(MidiEvent::new(offset, msg))
    }

    pub fn from_reaper(
        e: &'a reaper_medium::MidiEvent,
        sample_rate: Hz,
    ) -> Result<Self, &'static str> {
        let msg = IncomingMidiMessage::from_reaper(e.message())?;
        // Frame offset is given in 1/1024000 of a second, *not* sample frames!
        let offset = SampleOffset::from_midi_input_frame_offset(e.frame_offset(), sample_rate);
        Ok(MidiEvent::new(offset, msg))
    }
}

impl<'a> IncomingMidiMessage<'a> {
    pub fn from_vst(e: vst::event::Event<'a>) -> Result<Self, &'static str> {
        let res = match e {
            vst::event::Event::Midi(e) => {
                let short_msg = RawShortMessage::from_bytes((
                    e.data[0],
                    e.data[1]
                        .try_into()
                        .map_err(|_| "first data byte invalid")?,
                    e.data[2]
                        .try_into()
                        .map_err(|_| "second data byte invalid")?,
                ));
                let short_msg = short_msg.map_err(|_| "invalid status byte")?;
                IncomingMidiMessage::Short(short_msg)
            }
            vst::event::Event::SysEx(e) => IncomingMidiMessage::SysEx(e.payload),
            vst::event::Event::Deprecated(_) => return Err("deprecated message"),
        };
        Ok(res)
    }

    pub fn from_reaper(m: &'a reaper_medium::MidiMessage) -> Result<Self, &'static str> {
        let short = m.to_short_message().map_err(|_| "invalid short message")?;
        let res = if short.r#type() == ShortMessageType::SystemExclusiveStart {
            Self::SysEx(m.as_slice())
        } else {
            Self::Short(short)
        };
        Ok(res)
    }

    fn to_owned(self) -> OwnedIncomingMidiMessage {
        use IncomingMidiMessage::*;
        match self {
            Short(m) => OwnedIncomingMidiMessage::Short(m),
            SysEx(m) => OwnedIncomingMidiMessage::SysEx(m.to_owned()),
        }
    }

    fn might_be_automatically_generated_by_reaper(&self) -> bool {
        match self {
            // TODO-medium Maybe also filter all-sound-off (v6.36+dev0920).
            IncomingMidiMessage::Short(m) => m.r#type() == ShortMessageType::NoteOff,
            IncomingMidiMessage::SysEx(_) => false,
        }
    }

    fn to_source_value(self) -> MidiSourceValue<'a, RawShortMessage> {
        use IncomingMidiMessage::*;
        match self {
            Short(msg) => MidiSourceValue::Plain(msg),
            SysEx(msg) => MidiSourceValue::BorrowedSysEx(msg),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct AudioBlockProps {
    pub block_length: usize,
    pub frame_rate: Hz,
}

impl AudioBlockProps {
    pub fn from_vst(buffer: &vst::buffer::AudioBuffer<f64>, sample_rate: Hz) -> Self {
        Self {
            block_length: buffer.samples(),
            frame_rate: sample_rate,
        }
    }

    pub fn from_on_audio_buffer_args(args: &OnAudioBufferArgs) -> Self {
        Self {
            block_length: args.len as _,
            frame_rate: args.srate,
        }
    }

    #[cfg(feature = "playtime")]
    pub fn to_playtime(self) -> playtime_clip_engine::rt::BasicAudioRequestProps {
        playtime_clip_engine::rt::BasicAudioRequestProps {
            block_length: self.block_length,
            frame_rate: self.frame_rate,
        }
    }
}

fn log_virtual_control_input(
    sender: &SenderToNormalThread<ControlMainTask>,
    value: ControlEvent<VirtualSourceValue>,
    match_outcome: MatchOutcome,
) {
    sender.send_complaining(ControlMainTask::LogVirtualControlInput {
        event: value,
        match_outcome,
    });
}

fn flatten_control_midi_event<T: Copy>(evt: ControlEvent<MidiEvent<T>>) -> ControlEvent<T> {
    // TODO-medium We could have sample-accurate control event times by converting the MIDI event
    //  sample offset to something like microseconds (according to the current sample rate or by
    //  using REAPER's MidiFrameOffset type instead of SampleOffset in the first place) and using
    //  this microsecond unit in ControlEvent time.
    evt.map_payload(|midi_evt| midi_evt.payload())
}

#[derive(Copy, Clone)]
pub struct LogOptions {
    pub virtual_input_logging_enabled: bool,
    pub output_logging_enabled: bool,
    pub target_control_logging_enabled: bool,
}

impl LogOptions {
    fn from_basic_settings(settings: &BasicSettings) -> Self {
        LogOptions {
            virtual_input_logging_enabled: settings.virtual_input_logging_enabled,
            output_logging_enabled: settings.real_output_logging_enabled,
            target_control_logging_enabled: settings.target_control_logging_enabled,
        }
    }
}

fn is_rendering() -> bool {
    Reaper::get()
        .medium_reaper()
        .enum_projects(ProjectRef::CurrentlyRendering, 0)
        .is_some()
}
