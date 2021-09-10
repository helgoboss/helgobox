use crate::domain::{
    classify_midi_message, CompoundMappingSource, ControlMainTask, ControlMode, ControlOptions,
    Event, FeedbackSendBehavior, Garbage, GarbageBin, InputMatchResult, InstanceId,
    LifecycleMidiMessage, LifecyclePhase, MappingCompartment, MappingId, MidiClockCalculator,
    MidiMessageClassification, MidiSource, MidiSourceScanner, NormalRealTimeToMainThreadTask,
    OrderedMappingMap, PartialControlMatch, PersistentMappingProcessingState, QualifiedMappingId,
    RealTimeCompoundMappingTarget, RealTimeMapping, RealTimeReaperTarget, SampleOffset,
    SendMidiDestination, VirtualSourceValue,
};
use helgoboss_learn::{ControlValue, MidiSourceValue, RawMidiEvent};
use helgoboss_midi::{
    Channel, ControlChange14BitMessage, ControlChange14BitMessageScanner, DataEntryByteOrder,
    ParameterNumberMessage, PollingParameterNumberMessageScanner, RawShortMessage, ShortMessage,
};
use reaper_high::{MidiOutputDevice, Reaper};
use reaper_medium::{Hz, MidiInputDeviceId, MidiOutputDeviceId, SendMidiTime};
use slog::{debug, trace};

use crate::base::Global;
use assert_no_alloc::permit_alloc;
use enum_map::{enum_map, EnumMap};
use std::ptr::null_mut;
use std::time::Duration;
use vst::api::{EventType, Events, SysExEvent};
use vst::host::Host;
use vst::plugin::HostCallback;

const NORMAL_BULK_SIZE: usize = 100;
const FEEDBACK_BULK_SIZE: usize = 100;

#[derive(Debug)]
pub struct RealTimeProcessor {
    instance_id: InstanceId,
    logger: slog::Logger,
    // Synced processing settings
    control_mode: ControlMode,
    midi_control_input: MidiControlInput,
    midi_feedback_output: Option<MidiDestination>,
    mappings: EnumMap<MappingCompartment, OrderedMappingMap<RealTimeMapping>>,
    let_matched_events_through: bool,
    let_unmatched_events_through: bool,
    // State
    control_is_globally_enabled: bool,
    feedback_is_globally_enabled: bool,
    // Inter-thread communication
    normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
    feedback_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
    normal_main_task_sender: crossbeam_channel::Sender<NormalRealTimeToMainThreadTask>,
    control_main_task_sender: crossbeam_channel::Sender<ControlMainTask>,
    garbage_bin: GarbageBin,
    // Scanners for more complex MIDI message types
    nrpn_scanner: PollingParameterNumberMessageScanner,
    cc_14_bit_scanner: ControlChange14BitMessageScanner,
    // For source learning
    midi_source_scanner: MidiSourceScanner,
    // For MIDI timing clock calculations
    midi_clock_calculator: MidiClockCalculator,
    sample_rate: Hz,
    input_logging_enabled: bool,
    output_logging_enabled: bool,
}

impl RealTimeProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: InstanceId,
        parent_logger: &slog::Logger,
        normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
        feedback_task_sender: crossbeam_channel::Sender<FeedbackRealTimeTask>,
        normal_main_task_sender: crossbeam_channel::Sender<NormalRealTimeToMainThreadTask>,
        control_main_task_sender: crossbeam_channel::Sender<ControlMainTask>,
        garbage_bin: GarbageBin,
    ) -> RealTimeProcessor {
        use MappingCompartment::*;
        RealTimeProcessor {
            instance_id,
            logger: parent_logger.new(slog::o!("struct" => "RealTimeProcessor")),
            control_mode: ControlMode::Controlling,
            normal_task_receiver,
            feedback_task_receiver,
            feedback_task_sender,
            normal_main_task_sender,
            control_main_task_sender,
            mappings: enum_map! {
                ControllerMappings => ordered_map_with_capacity(1000),
                MainMappings => ordered_map_with_capacity(5000),
            },
            let_matched_events_through: false,
            let_unmatched_events_through: false,
            nrpn_scanner: PollingParameterNumberMessageScanner::new(Duration::from_millis(1)),
            cc_14_bit_scanner: Default::default(),
            midi_control_input: MidiControlInput::FxInput,
            midi_feedback_output: None,
            midi_source_scanner: Default::default(),
            midi_clock_calculator: Default::default(),
            control_is_globally_enabled: true,
            feedback_is_globally_enabled: true,
            garbage_bin,
            input_logging_enabled: false,
            output_logging_enabled: false,
            sample_rate: Hz::new(1.0),
        }
    }

    pub fn process_incoming_midi_from_vst(
        &mut self,
        event: Event<RawShortMessage>,
        is_reaper_generated: bool,
        host: &HostCallback,
    ) {
        if self.midi_control_input == MidiControlInput::FxInput {
            if is_reaper_generated {
                // Ignore note off messages which are a result of starting the transport. They
                // are generated by REAPER in order to stop instruments from sounding. But ReaLearn
                // is not an instrument in the classical sense. We don't want to reset target values
                // just because play has been pressed!
                self.process_unmatched_short(event, Caller::Vst(host));
                return;
            }
            self.process_incoming_midi(event, Caller::Vst(host));
        } else {
            // #33, #290 If MIDI input device is not set to <FX input>, we want to pass through all
            // messages.
            self.send_short_midi_to_fx_output(event, Caller::Vst(host))
        }
    }

    pub fn run_from_vst(&mut self, _sample_count: usize, host: &HostCallback) {
        if self.get_feedback_driver() == Driver::Vst {
            self.process_feedback_tasks(Caller::Vst(host));
        }
    }

    /// This should be regularly called by audio hook in normal mode.
    pub fn run_from_audio_hook_all(&mut self, sample_count: usize, might_be_rebirth: bool) {
        self.run_from_audio_hook_essential(sample_count, might_be_rebirth);
        self.run_from_audio_hook_control_and_learn();
    }

    pub fn midi_control_input(&self) -> MidiControlInput {
        self.midi_control_input
    }

    pub fn control_is_globally_enabled(&self) -> bool {
        self.control_is_globally_enabled
    }

    /// This should be called by audio hook in normal mode whenever it receives a MIDI message that
    /// is relevant *for this ReaLearn instance* (the input device is not checked again).
    ///
    /// Returns whether this message should be filtered out from the global MIDI stream.
    pub fn process_incoming_midi_from_audio_hook(&mut self, event: Event<RawShortMessage>) -> bool {
        let matched = self.process_incoming_midi(event, Caller::AudioHook);
        let let_through = (matched && self.let_matched_events_through)
            || (!matched && self.let_unmatched_events_through);
        !let_through
    }

    fn request_full_sync_and_discard_tasks_if_successful(&mut self) {
        if self
            .normal_main_task_sender
            .try_send(NormalRealTimeToMainThreadTask::FullResyncToRealTimeProcessorPlease)
            .is_ok()
        {
            // Requesting a full resync was successful so we can safely discard accumulated tasks.
            let discarded_normal_task_count = self
                .normal_task_receiver
                .try_iter()
                .map(|t| self.garbage_bin.dispose(Garbage::NormalRealTimeTask(t)))
                .count();
            let discarded_feedback_task_count = self
                .feedback_task_receiver
                .try_iter()
                .map(|t| self.garbage_bin.dispose(Garbage::FeedbackRealTimeTask(t)))
                .count();
            permit_alloc(|| {
                debug!(
                    self.logger,
                    "Successfully requested full sync. Discarded {} normal and {} feedback tasks.",
                    discarded_normal_task_count,
                    discarded_feedback_task_count
                );
            });
        } else {
            permit_alloc(|| {
                debug!(
                    self.logger,
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
    pub fn run_from_audio_hook_essential(&mut self, sample_count: usize, might_be_rebirth: bool) {
        // Increase MIDI clock calculator's sample counter
        self.midi_clock_calculator
            .increase_sample_counter_by(sample_count as u64);
        // Process occasional tasks sent from other thread (probably main thread)
        if might_be_rebirth {
            self.request_full_sync_and_discard_tasks_if_successful();
        }
        let normal_task_count = self.normal_task_receiver.len();
        for task in self.normal_task_receiver.try_iter().take(NORMAL_BULK_SIZE) {
            use NormalRealTimeTask::*;
            match task {
                UpdateControlIsGloballyEnabled(is_enabled) => {
                    self.control_is_globally_enabled = is_enabled;
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    // Handle lifecycle MIDI
                    if self.midi_feedback_output.is_some()
                        && is_enabled != self.feedback_is_globally_enabled
                    {
                        self.send_lifecycle_midi_for_all_mappings(is_enabled.into());
                    }
                    // Set
                    self.feedback_is_globally_enabled = is_enabled;
                }
                UpdateAllMappings(compartment, mut mappings) => {
                    permit_alloc(|| {
                        debug!(
                            self.logger,
                            "Updating {} {}...",
                            mappings.len(),
                            compartment
                        );
                    });
                    // Handle deactivation MIDI
                    if self.processor_feedback_is_effectively_on() {
                        self.send_lifecycle_midi_for_all_mappings_in(
                            compartment,
                            LifecyclePhase::Deactivation,
                        );
                    }
                    // Clear existing mappings (without deallocating)
                    for (_, m) in self.mappings[compartment].drain(..) {
                        self.garbage_bin.dispose_real_time_mapping(m);
                    }
                    // Set
                    let drained_mappings = mappings.drain(..).map(|m| (m.id(), m));
                    self.mappings[compartment].extend(drained_mappings);
                    self.garbage_bin
                        .dispose(Garbage::RealTimeMappings(mappings));
                    // Handle activation MIDI
                    if self.processor_feedback_is_effectively_on() {
                        self.send_lifecycle_midi_for_all_mappings_in(
                            compartment,
                            LifecyclePhase::Activation,
                        );
                    }
                }
                UpdateSingleMapping(compartment, mut mapping) => {
                    let m = std::mem::replace(&mut *mapping, None)
                        .expect("must send a mapping when updating single mapping");
                    self.garbage_bin
                        .dispose(Garbage::BoxedRealTimeMapping(mapping));
                    permit_alloc(|| {
                        debug!(
                            self.logger,
                            "Updating single {} {:?}...",
                            compartment,
                            m.id()
                        );
                    });
                    // Send lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        let was_on_before = self.mappings[compartment]
                            .get(&m.id())
                            .map_or(false, |m| m.feedback_is_effectively_on());
                        let is_on_now = m.feedback_is_effectively_on();
                        self.send_lifecycle_midi_diff(&m, was_on_before, is_on_now)
                    }
                    // Update
                    let old_mapping = self.mappings[compartment].insert(m.id(), m);
                    if let Some(m) = old_mapping {
                        self.garbage_bin.dispose_real_time_mapping(m);
                    }
                }
                UpdatePersistentMappingProcessingState { id, state } => {
                    permit_alloc(|| {
                        debug!(
                            self.logger,
                            "Updating persistent state of {} {:?}...", id.compartment, id.id
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
                UpdateTargetActivations(compartment, activation_updates) => {
                    // Also log sample count in order to be sure about invocation order
                    // (timestamp is not accurate enough on e.g. selection changes).
                    // TODO-low We should use an own logger and always log the sample count
                    //  automatically.
                    permit_alloc(|| {
                        debug!(
                            self.logger,
                            "Update target activations in {} at {} samples...",
                            compartment,
                            self.midi_clock_calculator.current_sample_count()
                        );
                    });
                    // Apply updates
                    for update in activation_updates.iter() {
                        if let Some(m) = self.mappings[compartment].get_mut(&update.id) {
                            m.update_target_activation(update.is_active);
                        }
                    }
                    // Handle lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        for update in activation_updates.iter() {
                            if let Some(m) = self.mappings[compartment].get(&update.id) {
                                if m.feedback_is_effectively_on_ignoring_target_activation() {
                                    self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                                        m,
                                        update.is_active.into(),
                                    );
                                }
                            }
                        }
                    }
                    self.garbage_bin
                        .dispose(Garbage::ActivationChanges(activation_updates));
                }
                UpdateSettings {
                    let_matched_events_through,
                    let_unmatched_events_through,
                    midi_control_input,
                    midi_feedback_output,
                    input_logging_enabled,
                    output_logging_enabled,
                } => {
                    permit_alloc(|| {
                        debug!(self.logger, "Updating settings...");
                    });
                    let feedback_output_changing =
                        midi_feedback_output != self.midi_feedback_output;
                    // Handle deactivation
                    if self.processor_feedback_is_effectively_on() && feedback_output_changing {
                        self.send_lifecycle_midi_for_all_mappings(LifecyclePhase::Deactivation);
                    }
                    // Update settings
                    self.let_matched_events_through = let_matched_events_through;
                    self.let_unmatched_events_through = let_unmatched_events_through;
                    self.midi_control_input = midi_control_input;
                    self.midi_feedback_output = midi_feedback_output;
                    self.input_logging_enabled = input_logging_enabled;
                    self.output_logging_enabled = output_logging_enabled;
                    // Handle activation
                    if self.processor_feedback_is_effectively_on() && feedback_output_changing {
                        self.send_lifecycle_midi_for_all_mappings(LifecyclePhase::Activation);
                    }
                }
                UpdateSampleRate(sample_rate) => {
                    permit_alloc(|| {
                        debug!(self.logger, "Updating sample rate");
                    });
                    self.sample_rate = sample_rate;
                    self.midi_clock_calculator.update_sample_rate(sample_rate);
                }
                StartLearnSource {
                    allow_virtual_sources,
                } => {
                    permit_alloc(|| {
                        debug!(self.logger, "Start learning source");
                    });
                    self.control_mode = ControlMode::LearningSource {
                        allow_virtual_sources,
                        osc_arg_index_hint: None,
                    };
                    self.midi_source_scanner.reset();
                }
                DisableControl => {
                    permit_alloc(|| {
                        debug!(self.logger, "Disable control");
                    });
                    self.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    permit_alloc(|| {
                        debug!(self.logger, "Return to control mode");
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
                UpdateMappingActivations(compartment, activation_updates) => {
                    permit_alloc(|| {
                        debug!(self.logger, "Updating mapping activations...");
                    });
                    // Apply updates
                    for update in activation_updates.iter() {
                        if let Some(m) = self.mappings[compartment].get_mut(&update.id) {
                            m.update_activation(update.is_active);
                        }
                    }
                    // Handle lifecycle MIDI
                    if self.processor_feedback_is_effectively_on() {
                        for update in activation_updates.iter() {
                            if let Some(m) = self.mappings[compartment].get(&update.id) {
                                if m.feedback_is_effectively_on_ignoring_mapping_activation() {
                                    self.send_lifecycle_midi_to_feedback_output_from_audio_hook(
                                        m,
                                        update.is_active.into(),
                                    );
                                }
                            }
                        }
                    }
                    self.garbage_bin
                        .dispose(Garbage::ActivationChanges(activation_updates));
                }
            }
        }
        // It's better to send feedback after processing the settings update - otherwise there's the
        // danger that feedback it sent to the wrong device or not at all.
        if self.get_feedback_driver() == Driver::AudioHook {
            self.process_feedback_tasks(Caller::AudioHook);
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
        self.feedback_is_globally_enabled && self.midi_feedback_output.is_some()
    }

    fn send_lifecycle_midi_for_all_mappings(&self, phase: LifecyclePhase) {
        for compartment in MappingCompartment::enum_iter() {
            self.send_lifecycle_midi_for_all_mappings_in(compartment, phase);
        }
    }

    fn send_lifecycle_midi_for_all_mappings_in(
        &self,
        compartment: MappingCompartment,
        phase: LifecyclePhase,
    ) {
        for m in self.mappings[compartment].values() {
            if m.feedback_is_effectively_on() {
                self.send_lifecycle_midi_to_feedback_output_from_audio_hook(m, phase);
            }
        }
    }

    /// This should *not* be called by the global audio hook when it's globally learning sources
    /// because we want to pause controlling in that case!
    fn run_from_audio_hook_control_and_learn(&mut self) {
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
                            let event = Event::without_offset(nrpn_msg);
                            self.process_incoming_midi_normal_nrpn(event, Caller::AudioHook);
                        }
                    }
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                ..
            } => {
                // Poll source scanner if we are learning a source currently (local learning)
                if let Some((source, _)) = self.midi_source_scanner.poll() {
                    self.learn_source(source, allow_virtual_sources);
                }
            }
        }
    }

    /// There's an important difference between using audio hook or VST plug-in as driver:
    /// VST processing stops e.g. when project paused and track not armed or on input FX chain and
    /// track not armed. The result is that control, feedback, mapping updates and many other things
    /// wouldn't work anymore. That's why we prefer audio hook whenever possible. However, we can't
    /// use the audio hook if we need access to the VST plug-in host callback because it's dangerous
    /// (would crash when plug-in gone) and somehow strange (although it seems to work).
    ///
    /// **IMPORTANT**: If "MIDI control input" is set to a MIDI device, it's very important that
    /// `run()` is called either just from the VST or just from the audio hook. If both do it,
    /// the MIDI messages are processed **twice**!!! Easy solution: Never have two drivers.
    fn get_feedback_driver(&self) -> Driver {
        use Driver::*;
        match self.midi_feedback_output {
            // Feedback not sent at all. We still want to "eat" any remaining feedback messages.
            // We do everything in the audio hook because it's more reliable.
            None => AudioHook,
            // Feedback sent directly to device. Same here: We let the audio hook do everything in
            // order to not run into surprising situations where control or feedback don't work.
            Some(MidiDestination::Device(_)) => AudioHook,
            // Feedback sent to FX output. Here we have to be more careful because sending feedback
            // to FX output involves host callback invocation. This can only be done from the VST
            // plug-in.
            Some(MidiDestination::FxOutput) => Vst,
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
            - State: {:?} \n\
            - Total main mapping count: {} \n\
            - Enabled main mapping count: {} \n\
            - Total controller mapping count: {} \n\
            - Enabled controller mapping count: {} \n\
            - Normal task count: {} \n\
            - Feedback task count: {} \n\
            ",
                self.control_mode,
                self.mappings[MappingCompartment::MainMappings].len(),
                self.mappings[MappingCompartment::MainMappings]
                    .values()
                    .filter(|m| m.control_is_effectively_on())
                    .count(),
                self.mappings[MappingCompartment::ControllerMappings].len(),
                self.mappings[MappingCompartment::ControllerMappings]
                    .values()
                    .filter(|m| m.control_is_effectively_on())
                    .count(),
                task_count,
                self.feedback_task_receiver.len(),
            );
            Global::task_support()
                .do_in_main_thread_asap(move || {
                    Reaper::get().show_console_msg(msg);
                })
                .unwrap();
            // Detailled
            trace!(
                self.logger,
                "\n\
            # Real-time processor\n\
            \n\
            {:#?}
            ",
                self
            );
        });
    }

    fn log_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        permit_alloc(|| {
            let mapping = self.mappings[compartment].get(&mapping_id);
            let msg = format!(
                "\n\
            # Real-time processor\n\
            \n\
            Mapping with ID {}:\n\
            {:#?}
            ",
                mapping_id, mapping
            );
            Global::task_support()
                .do_in_main_thread_asap(move || {
                    Reaper::get().show_console_msg(msg);
                })
                .unwrap();
        });
    }

    /// Returns if this MIDI event matched somehow.
    fn process_incoming_midi(&mut self, event: Event<RawShortMessage>, caller: Caller) -> bool {
        use MidiMessageClassification::*;
        match classify_midi_message(event.payload()) {
            Normal => self.process_incoming_midi_normal(event, caller),
            Ignored => {
                // ReaLearn doesn't process those. Forward them if user wants it.
                self.process_unmatched_short(event, caller);
                false
            }
            Timing => {
                // Timing clock messages are treated special (calculates BPM).
                // This is control-only, we never learn it.
                if self.control_is_globally_enabled {
                    if let Some(bpm) = self.midi_clock_calculator.feed(event.offset()) {
                        let source_value = MidiSourceValue::<RawShortMessage>::Tempo(bpm);
                        self.control_midi(Event::new(event.offset(), &source_value), caller)
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }
    }

    /// This basically splits the stream of short MIDI messages into 3 streams:
    ///
    /// - (N)RPN messages
    /// - 14-bit CC messages
    /// - Short MIDI messaages
    ///
    /// Returns whether the event somehow matched.
    fn process_incoming_midi_normal(
        &mut self,
        event: Event<RawShortMessage>,
        caller: Caller,
    ) -> bool {
        match self.control_mode {
            ControlMode::Controlling => {
                if self.control_is_globally_enabled {
                    // Even if an composite message ((N)RPN or CC 14-bit) was scanned, we still
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
                    let matched_or_consumed_plain =
                        self.process_incoming_midi_normal_plain(event, caller);
                    let matched_nrpn =
                        if let Some(nrpn_msg) = self.nrpn_scanner.feed(&event.payload()) {
                            let nrpn_event = Event::new(event.offset(), nrpn_msg);
                            self.process_incoming_midi_normal_nrpn(nrpn_event, caller)
                        } else {
                            false
                        };
                    let matched_cc14 =
                        if let Some(cc14_msg) = self.cc_14_bit_scanner.feed(&event.payload()) {
                            let cc14_event = Event::new(event.offset(), cc14_msg);
                            self.process_incoming_midi_normal_cc14(cc14_event, caller)
                        } else {
                            false
                        };
                    matched_or_consumed_plain || matched_nrpn || matched_cc14
                } else {
                    false
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                ..
            } => {
                if self.input_logging_enabled {
                    self.log_learn_input(event.payload());
                }
                if let Some(source) = self.midi_source_scanner.feed_short(event.payload(), None) {
                    self.learn_source(source, allow_virtual_sources);
                }
                true
            }
            ControlMode::Disabled => {
                // "Disabled" means we use this for global learning! We consider this therefore as
                // matched.
                true
            }
        }
    }

    /// Returns whether this message matched.
    fn process_incoming_midi_normal_nrpn(
        &mut self,
        event: Event<ParameterNumberMessage>,
        caller: Caller,
    ) -> bool {
        let source_value = MidiSourceValue::<RawShortMessage>::ParameterNumber(event.payload());
        let matched = self.control_midi(Event::new(event.offset(), &source_value), caller);
        if self.input_logging_enabled {
            self.log_control_input(source_value, false, matched);
        }
        if self.midi_control_input == MidiControlInput::FxInput
            && ((matched && self.let_matched_events_through)
                || (!matched && self.let_unmatched_events_through))
        {
            for m in event
                .payload()
                .to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst)
                .iter()
                .flatten()
            {
                self.send_short_midi_to_fx_output(Event::new(event.offset(), *m), caller);
            }
        }
        matched
    }

    fn log_control_input(
        &self,
        msg: MidiSourceValue<RawShortMessage>,
        consumed: bool,
        matched: bool,
    ) {
        self.control_main_task_sender
            .try_send(ControlMainTask::LogControlInput {
                value: msg,
                match_result: if consumed {
                    InputMatchResult::Consumed
                } else if matched {
                    InputMatchResult::Matched
                } else {
                    InputMatchResult::Unmatched
                },
            })
            .unwrap();
    }

    fn log_learn_input(&self, msg: RawShortMessage) {
        self.control_main_task_sender
            .try_send(ControlMainTask::LogLearnInput { msg })
            .unwrap();
    }

    fn log_lifecycle_output(&self, value: MidiSourceValue<RawShortMessage>) {
        self.normal_main_task_sender
            .try_send(NormalRealTimeToMainThreadTask::LogLifecycleOutput { value })
            .unwrap();
    }

    fn learn_source(&mut self, source: MidiSource, allow_virtual_sources: bool) {
        // If plug-in dropped, the receiver might be gone already because main processor is
        // unregistered synchronously.
        let _ = self.normal_main_task_sender.try_send(
            NormalRealTimeToMainThreadTask::LearnMidiSource {
                source,
                allow_virtual_sources,
            },
        );
    }

    /// Returns whether this message matched.
    fn process_incoming_midi_normal_cc14(
        &mut self,
        event: Event<ControlChange14BitMessage>,
        caller: Caller,
    ) -> bool {
        let source_value = MidiSourceValue::<RawShortMessage>::ControlChange14Bit(event.payload());
        let matched = self.control_midi(Event::new(event.offset(), &source_value), caller);
        if self.input_logging_enabled {
            self.log_control_input(source_value, false, matched);
        }
        if self.midi_control_input == MidiControlInput::FxInput
            && ((matched && self.let_matched_events_through)
                || (!matched && self.let_unmatched_events_through))
        {
            for m in event
                .payload()
                .to_short_messages::<RawShortMessage>()
                .iter()
            {
                let short_event = Event::new(event.offset(), *m);
                self.send_short_midi_to_fx_output(short_event, caller);
            }
        }
        matched
    }

    /// Returns whether this message matched or was at least consumed
    /// (e.g. as part of a NRPN message).
    fn process_incoming_midi_normal_plain(
        &mut self,
        event: Event<RawShortMessage>,
        caller: Caller,
    ) -> bool {
        let source_value = MidiSourceValue::Plain(event.payload());
        if self.is_consumed_by_at_least_one_source(event.payload()) {
            if self.input_logging_enabled {
                self.log_control_input(source_value, true, false);
            }
            // Some short MIDI messages are just parts of bigger composite MIDI messages,
            // e.g. (N)RPN or 14-bit CCs. If we reach this point, the incoming message
            // could potentially match one of the (N)RPN or 14-bit CC mappings in the list
            // and therefore doesn't qualify anymore as a candidate for normal CC sources.
            return true;
        }
        let matched = self.control_midi(Event::new(event.offset(), &source_value), caller);
        if self.input_logging_enabled {
            self.log_control_input(source_value, false, matched);
        }
        if matched {
            self.process_matched_short(event, caller);
        } else {
            self.process_unmatched_short(event, caller);
        }
        matched
    }

    fn all_mappings(&self) -> impl Iterator<Item = &RealTimeMapping> {
        MappingCompartment::enum_iter()
            .map(move |compartment| self.mappings[compartment].values())
            .flatten()
    }

    /// Returns whether this source value matched one of the mappings.
    fn control_midi(
        &mut self,
        value_event: Event<&MidiSourceValue<RawShortMessage>>,
        caller: Caller,
    ) -> bool {
        // We do pattern matching in order to use Rust's borrow splitting.
        let matched_controller = if let [ref mut controller_mappings, ref mut main_mappings] =
            self.mappings.as_mut_slice()
        {
            control_controller_mappings_midi(
                &self.control_main_task_sender,
                controller_mappings,
                main_mappings,
                value_event,
                caller,
                self.midi_feedback_output,
                self.output_logging_enabled,
            )
        } else {
            unreachable!()
        };
        let matched_main = self.control_main_mappings_midi(value_event, caller);
        matched_main || matched_controller
    }

    fn control_main_mappings_midi(
        &mut self,
        source_value_event: Event<&MidiSourceValue<RawShortMessage>>,
        caller: Caller,
    ) -> bool {
        let compartment = MappingCompartment::MainMappings;
        let mut matched = false;
        for m in self.mappings[compartment]
            .values_mut()
            // The UI prevents creating main mappings with virtual targets but a JSON import
            // doesn't. Check again that it's a REAPER target.
            .filter(|m| m.control_is_effectively_on() && m.has_reaper_target())
        {
            if let CompoundMappingSource::Midi(s) = &m.source() {
                if let Some(control_value) = s.control(source_value_event.payload()) {
                    let _ = process_real_mapping(
                        m,
                        &self.control_main_task_sender,
                        compartment,
                        Event::new(source_value_event.offset(), control_value),
                        ControlOptions {
                            enforce_target_refresh: matched,
                            ..Default::default()
                        },
                        caller,
                        self.midi_feedback_output,
                        self.output_logging_enabled,
                    );
                    matched = true;
                }
            }
        }
        matched
    }

    fn process_matched_short(&self, event: Event<RawShortMessage>, caller: Caller) {
        if self.midi_control_input != MidiControlInput::FxInput {
            return;
        }
        if !self.let_matched_events_through {
            return;
        }
        self.send_short_midi_to_fx_output(event, caller);
    }

    fn process_unmatched_short(&self, event: Event<RawShortMessage>, caller: Caller) {
        if self.midi_control_input != MidiControlInput::FxInput {
            return;
        }
        if !self.let_unmatched_events_through {
            return;
        }
        self.send_short_midi_to_fx_output(event, caller);
    }

    fn is_consumed_by_at_least_one_source(&self, msg: RawShortMessage) -> bool {
        self.all_mappings()
            .any(|m| m.control_is_effectively_on() && m.consumes(msg))
    }

    fn send_midi_feedback(&self, value: MidiSourceValue<RawShortMessage>, caller: Caller) {
        if let MidiSourceValue::Raw(msg) = value {
            send_raw_midi_to_fx_output(&msg, SampleOffset::ZERO, caller);
            self.garbage_bin.dispose(Garbage::RawMidiEvent(msg));
        } else {
            let shorts = value.to_short_messages(DataEntryByteOrder::MsbFirst);
            if shorts[0].is_none() {
                return;
            }
            for short in shorts.iter().flatten() {
                self.send_short_midi_to_fx_output(Event::without_offset(*short), caller);
            }
        }
    }

    fn send_lifecycle_midi_to_feedback_output_from_audio_hook(
        &self,
        m: &RealTimeMapping,
        phase: LifecyclePhase,
    ) {
        if let Some(output) = self.midi_feedback_output {
            match output {
                MidiDestination::FxOutput => {
                    // We can't send it now because we don't have safe access to the host callback
                    // because this method is being called from the audio hook.
                    let _ = self.feedback_task_sender.try_send(
                        FeedbackRealTimeTask::SendLifecycleMidi(m.compartment(), m.id(), phase),
                    );
                }
                MidiDestination::Device(dev_id) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            for m in m.lifecycle_midi_messages(phase) {
                                match m {
                                    LifecycleMidiMessage::Short(msg) => {
                                        if self.output_logging_enabled {
                                            self.log_lifecycle_output(MidiSourceValue::Plain(*msg));
                                        }
                                        mo.send(*msg, SendMidiTime::Instantly);
                                    }
                                    LifecycleMidiMessage::Raw(data) => {
                                        if self.output_logging_enabled {
                                            permit_alloc(|| {
                                                self.log_lifecycle_output(MidiSourceValue::Raw(
                                                    data.clone(),
                                                ));
                                            });
                                        }
                                        mo.send_msg(&**data, SendMidiTime::Instantly);
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
                    if self.output_logging_enabled {
                        self.log_lifecycle_output(MidiSourceValue::Plain(*msg));
                    }
                    self.send_short_midi_to_fx_output(Event::without_offset(*msg), caller)
                }
                LifecycleMidiMessage::Raw(data) => {
                    if self.output_logging_enabled {
                        permit_alloc(|| {
                            self.log_lifecycle_output(MidiSourceValue::Raw(data.clone()));
                        });
                    }
                    send_raw_midi_to_fx_output(data, SampleOffset::ZERO, caller)
                }
            }
        }
    }

    fn send_short_midi_to_fx_output(&self, event: Event<RawShortMessage>, caller: Caller) {
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
        system_data: bytes as *const _ as _,
        _reserved2: 0,
    }
}

fn build_short_midi_vst_event(event: Event<RawShortMessage>) -> vst::api::MidiEvent {
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
enum Caller<'a> {
    Vst(&'a HostCallback),
    AudioHook,
}

#[derive(Debug)]
pub struct RealTimeSender<T> {
    sender: crossbeam_channel::Sender<T>,
}

impl<T> Clone for RealTimeSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T> RealTimeSender<T> {
    pub fn new(sender: crossbeam_channel::Sender<T>) -> Self {
        assert!(
            sender.capacity().is_some(),
            "real-time sender channel must be bounded!"
        );
        Self { sender }
    }

    pub fn send(&self, task: T) -> Result<(), crossbeam_channel::TrySendError<T>> {
        if Reaper::get().audio_is_running() {
            // Audio is running so sending should always work. If not, it's an unexpected error and
            // we must return it.
            self.sender.try_send(task)
        } else {
            // Audio is not running. Maybe this is just a very temporary outage or a short initial
            // non-running state.
            if self.channel_still_has_some_headroom() {
                // Channel still has some headroom, so we send the task in order to support a
                // temporary outage. This should not fail unless another sender has exhausted the
                // channel in the meanwhile. Even then, so what. See "else" branch.
                let _ = self.sender.send(task);
                Ok(())
            } else {
                // Channel has already accumulated lots of tasks. Don't send!
                // It's not bad if we don't send this task because the real-time processor will
                // not be able to process it anyway at the moment (it's not going to be called
                // because the audio engine is stopped). Fear not, ReaLearn's audio hook has logic
                // that detects a "rebirth" - the moment when the audio cycle starts again. In this
                // case it will request a full resync of everything so nothing should get lost
                // in theory.
                Ok(())
            }
        }
    }

    fn channel_still_has_some_headroom(&self) -> bool {
        self.sender.len() <= self.sender.capacity().unwrap() / 2
    }
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalRealTimeTask {
    UpdateAllMappings(MappingCompartment, Vec<RealTimeMapping>),
    UpdateSingleMapping(MappingCompartment, Box<Option<RealTimeMapping>>),
    UpdatePersistentMappingProcessingState {
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    },
    UpdateSettings {
        let_matched_events_through: bool,
        let_unmatched_events_through: bool,
        midi_control_input: MidiControlInput,
        midi_feedback_output: Option<MidiDestination>,
        input_logging_enabled: bool,
        output_logging_enabled: bool,
    },
    /// This takes care of propagating target activation states (for non-virtual mappings).
    UpdateTargetActivations(MappingCompartment, Vec<ActivationChange>),
    /// Updates the activation state of multiple mappings.
    ///
    /// The given vector contains updates just for affected mappings. This is because when a
    /// parameter update occurs we can determine in a very granular way which targets are affected.
    UpdateMappingActivations(MappingCompartment, Vec<ActivationChange>),
    LogDebugInfo,
    LogMapping(MappingCompartment, MappingId),
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
    pub id: MappingId,
    pub is_active: bool,
}

/// A feedback task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackRealTimeTask {
    // When it comes to MIDI feedback, the real-time processor is only responsible for FX output
    // feedback. Direct-device feedback is taken care of by the global audio hook for reasons of
    // proper ordering.
    FxOutputFeedback(MidiSourceValue<RawShortMessage>),
    // Used only if feedback output is <FX output>, otherwise done synchronously.
    SendLifecycleMidi(MappingCompartment, MappingId, LifecyclePhase),
}

impl Drop for RealTimeProcessor {
    fn drop(&mut self) {
        permit_alloc(|| {
            debug!(self.logger, "Dropping real-time processor...");
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

fn control_controller_mappings_midi(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    // Mappings with virtual targets
    controller_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    // Mappings with virtual sources
    main_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    value_event: Event<&MidiSourceValue<RawShortMessage>>,
    caller: Caller,
    midi_feedback_output: Option<MidiDestination>,
    output_logging_enabled: bool,
) -> bool {
    let mut matched = false;
    let mut enforce_target_refresh = false;
    for m in controller_mappings
        .values_mut()
        .filter(|m| m.control_is_effectively_on())
    {
        if let Some(control_match) = m.control_midi_virtualizing(value_event.payload()) {
            use PartialControlMatch::*;
            let mapping_matched = match control_match {
                ProcessVirtual(virtual_source_value) => control_main_mappings_virtual(
                    sender,
                    main_mappings,
                    Event::new(value_event.offset(), virtual_source_value),
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
                    },
                    caller,
                    midi_feedback_output,
                    output_logging_enabled,
                ),
                ProcessDirect(control_value) => {
                    let _ = process_real_mapping(
                        m,
                        sender,
                        MappingCompartment::ControllerMappings,
                        Event::new(value_event.offset(), control_value),
                        ControlOptions {
                            enforce_target_refresh,
                            ..Default::default()
                        },
                        caller,
                        midi_feedback_output,
                        output_logging_enabled,
                    );
                    // We do this only for transactions of *real* targets matches.
                    enforce_target_refresh = true;
                    true
                }
            };
            if mapping_matched {
                matched = true;
            }
        }
    }
    matched
}

#[allow(clippy::too_many_arguments)]
fn process_real_mapping(
    mapping: &mut RealTimeMapping,
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    compartment: MappingCompartment,
    value_event: Event<ControlValue>,
    options: ControlOptions,
    caller: Caller,
    midi_feedback_output: Option<MidiDestination>,
    output_logging_enabled: bool,
) -> Result<(), &'static str> {
    if let Some(RealTimeCompoundMappingTarget::Reaper(reaper_target)) =
        mapping.resolved_target.as_mut()
    {
        // Must be processed here in real-time processor.
        let control_value: Option<ControlValue> = mapping
            .core
            .mode
            .control_with_options(
                value_event.payload(),
                reaper_target,
                (),
                options.mode_control_options,
            )
            .ok_or("mode didn't return control value")?
            .into();
        let v = control_value
            .ok_or("target already has desired value")?
            .to_absolute_value()?;
        match reaper_target {
            RealTimeReaperTarget::SendMidi(t) => {
                // This is a type of mapping that we should process right here because we want to
                // send a MIDI message and this needs to happen in the audio thread.
                // Going to the main thread and back would be such a waste!
                let raw_midi_event = t.pattern().to_concrete_midi_event(v);
                let midi_destination = match caller {
                    Caller::Vst(_) => match t.destination() {
                        SendMidiDestination::FxOutput => Some(MidiDestination::FxOutput),
                        SendMidiDestination::FeedbackOutput => {
                            Some(midi_feedback_output.ok_or("no feedback output set")?)
                        }
                    },
                    Caller::AudioHook => {
                        match t.destination() {
                            SendMidiDestination::FxOutput => {
                                // Control input = Device | Destination = FX output.
                                // Not supported currently. It could be by introducing a new
                                // `FxOutputTask` with a SendMidiToFxOutput variant, tasks being
                                // processed by `run_from_vst()` only no matter the feedback driver.
                                None
                            }
                            SendMidiDestination::FeedbackOutput => {
                                Some(midi_feedback_output.ok_or("no feedback output set")?)
                            }
                        }
                    }
                };
                if output_logging_enabled && midi_destination.is_some() {
                    permit_alloc(|| {
                        sender
                            .try_send(ControlMainTask::LogTargetOutput {
                                event: Box::new(raw_midi_event),
                            })
                            .unwrap();
                    });
                }
                let successful = match midi_destination {
                    Some(MidiDestination::FxOutput) => {
                        send_raw_midi_to_fx_output(&raw_midi_event, value_event.offset(), caller);
                        true
                    }
                    Some(MidiDestination::Device(dev_id)) => MidiOutputDevice::new(dev_id)
                        .with_midi_output(|mo| {
                            if let Some(mo) = mo {
                                mo.send_msg(&raw_midi_event, SendMidiTime::Instantly);
                                true
                            } else {
                                false
                            }
                        }),
                    _ => false,
                };
                if successful {
                    t.set_artificial_value(v);
                }
                Ok(())
            }
        }
    } else {
        forward_control_to_main_processor(
            sender,
            compartment,
            mapping.id(),
            value_event.payload(),
            options,
        );
        Ok(())
    }
}

fn forward_control_to_main_processor(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    value: ControlValue,
    options: ControlOptions,
) {
    let task = ControlMainTask::Control {
        compartment,
        mapping_id,
        value,
        options,
    };
    // If plug-in dropped, the receiver might be gone already because main processor is
    // unregistered synchronously.
    let _ = sender.try_send(task);
}

/// Returns whether this source value matched one of the mappings.
fn control_main_mappings_virtual(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    main_mappings: &mut OrderedMappingMap<RealTimeMapping>,
    value_event: Event<VirtualSourceValue>,
    options: ControlOptions,
    caller: Caller,
    midi_feedback_output: Option<MidiDestination>,
    output_logging_enabled: bool,
) -> bool {
    // Controller mappings can't have virtual sources, so for now we only need to check
    // main mappings.
    let mut matched = false;
    for m in main_mappings
        .values_mut()
        .filter(|m| m.control_is_effectively_on())
    {
        if let CompoundMappingSource::Virtual(s) = &m.source() {
            if let Some(control_value) = s.control(&value_event.payload()) {
                let _ = process_real_mapping(
                    m,
                    sender,
                    MappingCompartment::MainMappings,
                    Event::new(value_event.offset(), control_value),
                    ControlOptions {
                        enforce_target_refresh: matched,
                        ..options
                    },
                    caller,
                    midi_feedback_output,
                    output_logging_enabled,
                );
                matched = true;
            }
        }
    }
    matched
}

#[derive(Eq, PartialEq)]
enum Driver {
    AudioHook,
    Vst,
}

fn send_raw_midi_to_fx_output(data: &RawMidiEvent, offset: SampleOffset, caller: Caller) {
    let host = match caller {
        Caller::Vst(h) => h,
        _ => return,
    };
    let event = build_sysex_midi_vst_event(data.bytes(), offset);
    let events = build_vst_events(&event as *const _ as _);
    host.process_events(&events);
}

fn ordered_map_with_capacity<T>(cap: usize) -> OrderedMappingMap<T> {
    let mut map = OrderedMappingMap::with_capacity(cap);
    // This is a workaround for an indexmap bug which allocates space for entries on the
    // first extend/reserve call although it should have been done already when creating
    // it via with_capacity. Remember: We must not allocate in real-time thread!
    map.reserve(0);
    map
}
