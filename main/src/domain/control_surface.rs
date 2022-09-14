use crate::base::{metrics_util, Global, NamedChannelSender, SenderToNormalThread};
use crate::domain::{
    BackboneState, CompoundMappingSource, ControlEvent, ControlEventTimestamp,
    DeviceChangeDetector, DeviceControlInput, DeviceFeedbackOutput, DomainEventHandler,
    EelTransformation, FeedbackOutput, FeedbackRealTimeTask, FinalSourceFeedbackValue, InstanceId,
    LifecycleMidiData, MainProcessor, MidiCaptureSender, MidiDeviceChangePayload,
    MonitoringFxChainChangeDetector, NormalRealTimeTask, OscDeviceId, OscInputDevice,
    OscScanResult, QualifiedClipMatrixEvent, RealTimeCompoundMappingTarget, RealTimeMapping,
    RealTimeMappingUpdate, RealTimeTargetUpdate, ReaperConfigChangeDetector, ReaperMessage,
    ReaperTarget, SharedMainProcessors, SharedRealTimeProcessor, TouchedTrackParameterType,
};
use crossbeam_channel::Receiver;
use helgoboss_learn::{AbstractTimestamp, ModeGarbage, RawMidiEvents};
use reaper_high::{
    ChangeDetectionMiddleware, ChangeEvent, ControlSurfaceEvent, ControlSurfaceMiddleware,
    FutureMiddleware, Fx, FxParameter, MainTaskMiddleware, Project, Reaper,
};
use reaper_rx::ControlSurfaceRxMiddleware;
use rosc::{OscMessage, OscPacket};
use std::cell::RefCell;

use itertools::{EitherOrBoth, Itertools};
use playtime_clip_engine::rt::WeakMatrix;
use reaper_medium::{
    CommandId, ExtSupportsExtendedTouchArgs, GetTouchStateArgs, MediaTrack, MidiInputDeviceId,
    MidiOutputDeviceId, PositionInSeconds, ReaProject, ReaperNormalizedFxParamValue,
};
use rxrust::prelude::*;
use slog::debug;
use smallvec::SmallVec;
use std::collections::HashMap;

type OscCaptureSender = async_channel::Sender<OscScanResult>;

const CONTROL_SURFACE_MAIN_TASK_BULK_SIZE: usize = 10;
const ADDITIONAL_FEEDBACK_EVENT_BULK_SIZE: usize = 30;
const CLIP_MATRIX_EVENT_BULK_SIZE: usize = 30;
const INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE: usize = 30;
const OSC_INCOMING_BULK_SIZE: usize = 32;
const GARBAGE_BULK_SIZE: usize = 100;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    logger: slog::Logger,
    change_detection_middleware: ChangeDetectionMiddleware,
    change_event_queue: RefCell<Vec<ChangeEvent>>,
    monitoring_fx_chain_change_detector: MonitoringFxChainChangeDetector,
    rx_middleware: ControlSurfaceRxMiddleware,
    main_processors: SharedMainProcessors<EH>,
    main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
    clip_matrix_event_receiver: Receiver<QualifiedClipMatrixEvent>,
    additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
    instance_orchestration_event_receiver: Receiver<InstanceOrchestrationEvent>,
    main_task_middleware: MainTaskMiddleware,
    future_middleware: FutureMiddleware,
    counter: u64,
    full_beats: HashMap<ReaProject, u32>,
    state: State,
    osc_input_devices: Vec<OscInputDevice>,
    garbage_receiver: crossbeam_channel::Receiver<Garbage>,
    device_change_detector: DeviceChangeDetector,
    reaper_config_change_detector: ReaperConfigChangeDetector,
    control_surface_event_sender: SenderToNormalThread<ControlSurfaceEvent<'static>>,
    control_surface_event_receiver: crossbeam_channel::Receiver<ControlSurfaceEvent<'static>>,
}

pub enum Garbage {
    RawMidiEvents(RawMidiEvents),
    RealTimeProcessor(SharedRealTimeProcessor),
    LifecycleMidiData(LifecycleMidiData),
    ResolvedTarget(Option<RealTimeCompoundMappingTarget>),
    Mode(ModeGarbage<EelTransformation>),
    MappingSource(CompoundMappingSource),
    RealTimeMappings(Vec<RealTimeMapping>),
    BoxedRealTimeMapping(Box<Option<RealTimeMapping>>),
    MappingUpdates(Vec<RealTimeMappingUpdate>),
    TargetUpdates(Vec<RealTimeTargetUpdate>),
    NormalRealTimeTask(NormalRealTimeTask),
    FeedbackRealTimeTask(FeedbackRealTimeTask),
    MidiCaptureSender(MidiCaptureSender),
    ClipMatrix(WeakMatrix),
}

#[derive(Debug)]
enum State {
    Normal,
    CapturingOsc(OscCaptureSender),
    LearningTarget(async_channel::Sender<ReaperTarget>),
}

pub enum RealearnControlSurfaceMainTask<EH: DomainEventHandler> {
    // Removing a main processor is done synchronously by temporarily regaining ownership of the
    // control surface from REAPER.
    AddMainProcessor(MainProcessor<EH>),
    LogDebugInfo,
    StartLearningTargets(async_channel::Sender<ReaperTarget>),
    StartCapturingOsc(OscCaptureSender),
    StopCapturingOsc,
    SendAllFeedback,
}

/// Not all events in REAPER are communicated via a control surface, e.g. action invocations.
#[derive(Debug)]
pub enum AdditionalFeedbackEvent {
    ActionInvoked(ActionInvokedEvent),
    FxSnapshotLoaded(FxSnapshotLoadedEvent),
    /// Work around REAPER's inability to notify about parameter changes in
    /// monitoring FX by simulating the notification ourselves.
    /// Then parameter learning and feedback works at least for
    /// ReaLearn monitoring FX instances, which is especially
    /// useful for conditional activation.
    RealearnMonitoringFxParameterValueChanged(RealearnMonitoringFxParameterValueChangedEvent),
    ParameterAutomationTouchStateChanged(ParameterAutomationTouchStateChangedEvent),
    /// Beat-changed events are emitted only when the project is playing.
    ///
    /// We shouldn't change that because targets such as "Marker/region: Go to" or "Project: Seek"
    /// depend on this (see https://github.com/helgoboss/realearn/issues/663).
    BeatChanged(BeatChangedEvent),
}

#[derive(Debug)]
pub enum InstanceOrchestrationEvent {
    /// Sent by a ReaLearn instance X if it releases control over a source.
    ///
    /// This enables other instances to take over control of that source before X finally "switches
    /// off lights".
    SourceReleased(SourceReleasedEvent),
    /// Whenever something about instance's device usage changes (either input or output or both
    /// potentially change).
    IoUpdated(IoUpdatedEvent),
}

/// Communicates changes in which input and output device a ReaLearn instance uses or used.
#[derive(Debug)]
pub struct IoUpdatedEvent {
    pub instance_id: InstanceId,
    pub control_input: Option<DeviceControlInput>,
    pub control_input_used: bool,
    pub feedback_output: Option<DeviceFeedbackOutput>,
    pub feedback_output_used: bool,
    pub feedback_output_usage_might_have_changed: bool,
}

#[derive(Debug)]
pub struct SourceReleasedEvent {
    pub instance_id: InstanceId,
    pub feedback_output: FeedbackOutput,
    pub feedback_value: FinalSourceFeedbackValue,
}

#[derive(Debug)]
pub struct BeatChangedEvent {
    pub project: Project,
    pub new_value: PositionInSeconds,
}

#[derive(Debug)]
pub struct ActionInvokedEvent {
    pub command_id: CommandId,
}

#[derive(Debug)]
pub struct FxSnapshotLoadedEvent {
    pub fx: Fx,
}

#[derive(Debug)]
pub struct RealearnMonitoringFxParameterValueChangedEvent {
    pub parameter: FxParameter,
    pub new_value: ReaperNormalizedFxParamValue,
}

#[derive(Debug)]
pub struct ParameterAutomationTouchStateChangedEvent {
    pub track: MediaTrack,
    pub parameter_type: TouchedTrackParameterType,
    pub new_value: bool,
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent_logger: &slog::Logger,
        main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
        clip_matrix_event_receiver: Receiver<QualifiedClipMatrixEvent>,
        additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
        instance_orchestration_event_receiver: Receiver<InstanceOrchestrationEvent>,
        garbage_receiver: crossbeam_channel::Receiver<Garbage>,
        main_processors: SharedMainProcessors<EH>,
    ) -> Self {
        let logger = parent_logger.new(slog::o!("struct" => "RealearnControlSurfaceMiddleware"));
        let mut device_change_detector = DeviceChangeDetector::new();
        // Prevent change messages to be sent on load by polling one time and ignoring result.
        device_change_detector.poll_for_midi_input_device_changes();
        device_change_detector.poll_for_midi_output_device_changes();
        let (control_surface_event_sender, control_surface_event_receiver) =
            SenderToNormalThread::new_unbounded_channel("control surface events");
        Self {
            logger: logger.clone(),
            change_detection_middleware: ChangeDetectionMiddleware::new(),
            change_event_queue: RefCell::new(Vec::with_capacity(100)),
            monitoring_fx_chain_change_detector: Default::default(),
            rx_middleware: ControlSurfaceRxMiddleware::new(Global::control_surface_rx().clone()),
            main_processors,
            main_task_receiver,
            clip_matrix_event_receiver,
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            main_task_middleware: MainTaskMiddleware::new(
                logger.clone(),
                Global::get().task_sender(),
                Global::get().task_receiver(),
            ),
            future_middleware: FutureMiddleware::new(
                logger.clone(),
                Global::get().executor(),
                Global::get().local_executor(),
            ),
            counter: 0,
            full_beats: Default::default(),
            state: State::Normal,
            osc_input_devices: vec![],
            garbage_receiver,
            device_change_detector,
            reaper_config_change_detector: Default::default(),
            control_surface_event_sender,
            control_surface_event_receiver,
        }
    }

    pub fn remove_main_processor(&mut self, id: &InstanceId) {
        self.main_processors
            .borrow_mut()
            .retain(|p| p.instance_id() != id);
    }

    pub fn set_osc_input_devices(&mut self, devs: Vec<OscInputDevice>) {
        self.osc_input_devices = devs;
    }

    pub fn clear_osc_input_devices(&mut self) {
        self.osc_input_devices.clear();
    }

    /// Called when waking up ReaLearn (first instance appears again or the first time).
    pub fn wake_up(&self) {
        let mut change_events = vec![];
        self.change_detection_middleware.reset(|e| {
            change_events.push(e);
        });
        for m in &*self.main_processors.borrow() {
            m.process_control_surface_change_events(&change_events);
        }
        for e in change_events {
            self.rx_middleware.handle_change(e);
        }
        // We don't want to execute tasks which accumulated during the "downtime" of Reaper.
        // So we just consume all without executing them.
        self.main_task_middleware.reset();
        self.future_middleware.reset();
    }

    fn process_change_events(&mut self) {
        let mut normal_events = self.change_event_queue.borrow_mut();
        let monitoring_fx_events =
            metrics_util::measure_time("detect monitoring FX changes", || {
                self.monitoring_fx_chain_change_detector.poll_for_changes()
            });
        if normal_events.is_empty() && monitoring_fx_events.is_empty() {
            return;
        }
        // This is for feedback processing. No Rx!
        let main_processors = self.main_processors.borrow();
        for p in main_processors.iter() {
            if !normal_events.is_empty() {
                p.process_control_surface_change_events(&normal_events);
            }
            if !monitoring_fx_events.is_empty() {
                p.process_control_surface_change_events(&monitoring_fx_events);
            }
        }
        // The rest is only for upper layers (e.g. UI), not for processing.
        for e in normal_events
            .drain(..)
            .chain(monitoring_fx_events.into_iter())
        {
            self.rx_middleware.handle_change(e.clone());
            if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                // TODO-medium Now we have the necessary framework (AdditionalFeedbackEvent)
                //  to also support action, FX snapshot and ReaLearn monitoring FX parameter
                //  touching for "Last touched" target and global learning (see
                //  LearningTarget state)! Connect the dots!
                BackboneState::get().set_last_touched_target(target);
                for p in &*self.main_processors.borrow() {
                    p.notify_target_touched();
                }
            }
        }
    }

    fn process_deferred_control_surface_events(&self) {
        while let Ok(event) = self.control_surface_event_receiver.try_recv() {
            let mut change_event_queue = self.change_event_queue.borrow_mut();
            self.handle_event_internal(&event, &mut change_event_queue);
        }
    }

    fn process_main_tasks(&mut self) {
        for t in self
            .main_task_receiver
            .try_iter()
            .take(CONTROL_SURFACE_MAIN_TASK_BULK_SIZE)
        {
            use RealearnControlSurfaceMainTask::*;
            match t {
                AddMainProcessor(p) => {
                    self.main_processors.borrow_mut().push(p);
                }
                LogDebugInfo => {
                    self.log_debug_info();
                }
                StartLearningTargets(sender) => {
                    self.state = State::LearningTarget(sender);
                }
                StopCapturingOsc => {
                    self.state = State::Normal;
                }
                StartCapturingOsc(sender) => {
                    self.state = State::CapturingOsc(sender);
                }
                SendAllFeedback => {
                    for m in &*self.main_processors.borrow() {
                        m.send_all_feedback();
                    }
                }
            }
        }
    }

    fn drop_garbage(&mut self) {
        for garbage in self.garbage_receiver.try_iter().take(GARBAGE_BULK_SIZE) {
            let _ = garbage;
        }
    }

    fn poll_clip_matrixes(&mut self) {
        for processor in &*self.main_processors.borrow() {
            let events = processor.poll_owned_clip_matrix();
            if events.is_empty() {
                continue;
            }
            for other_processor in &*self.main_processors.borrow() {
                other_processor.process_clip_matrix_events(*processor.instance_id(), &events);
            }
        }
    }

    fn run_main_processors(&mut self, timestamp: ControlEventTimestamp) {
        match &self.state {
            State::Normal => {
                for p in &mut *self.main_processors.borrow_mut() {
                    p.run_essential(timestamp);
                    p.run_control(timestamp);
                }
            }
            State::CapturingOsc(_) | State::LearningTarget(_) => {
                for p in &mut *self.main_processors.borrow_mut() {
                    p.run_essential(timestamp);
                }
            }
        }
    }

    fn process_incoming_additional_feedback(&mut self) {
        for event in self
            .additional_feedback_event_receiver
            .try_iter()
            .take(ADDITIONAL_FEEDBACK_EVENT_BULK_SIZE)
        {
            if let AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(e) = &event {
                let rx = Global::control_surface_rx();
                rx.fx_parameter_value_changed
                    .borrow_mut()
                    .next(e.parameter.clone());
                rx.fx_parameter_touched
                    .borrow_mut()
                    .next(e.parameter.clone());
            }
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_additional_feedback_event(&event)
            }
        }
    }

    fn process_incoming_clip_matrix_events(&mut self) {
        for event in self
            .clip_matrix_event_receiver
            .try_iter()
            .take(CLIP_MATRIX_EVENT_BULK_SIZE)
        {
            for p in &mut *self.main_processors.borrow_mut() {
                p.process_clip_matrix_event(&event);
            }
        }
    }

    fn process_instance_orchestration_events(&mut self) {
        for event in self
            .instance_orchestration_event_receiver
            .try_iter()
            .take(INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE)
        {
            use InstanceOrchestrationEvent::*;
            match event {
                SourceReleased(e) => {
                    debug!(self.logger, "Source of instance {} released", e.instance_id);
                    // We also allow the instance to take over which released the source in
                    // the first place! Simply because in the meanwhile, this instance
                    // could have found a new usage for it! E.g. likely to happen with
                    // preset changes.
                    let other_instance_took_over = self
                        .main_processors
                        .borrow()
                        .iter()
                        .any(|p| p.maybe_takeover_source(&e));
                    if !other_instance_took_over {
                        if let Some(p) = self
                            .main_processors
                            .borrow()
                            .iter()
                            .find(|p| p.instance_id() == &e.instance_id)
                        {
                            // Finally safe to switch off lights!
                            p.finally_switch_off_source(e.feedback_output, e.feedback_value);
                        }
                    }
                }
                IoUpdated(e) => {
                    let backbone_state = BackboneState::get();
                    let feedback_dev_usage_changed = backbone_state.update_io_usage(
                        &e.instance_id,
                        if e.control_input_used {
                            e.control_input
                        } else {
                            None
                        },
                        if e.feedback_output_used {
                            e.feedback_output
                        } else {
                            None
                        },
                    );
                    if feedback_dev_usage_changed
                        && backbone_state.lives_on_upper_floor(&e.instance_id)
                    {
                        debug!(
                            self.logger,
                            "Upper-floor instance {} {} feedback output",
                            e.instance_id,
                            if e.feedback_output_used {
                                "claimed"
                            } else {
                                "released"
                            }
                        );
                        if let Some(feedback_output) = e.feedback_output {
                            // Give lower-floor instances the chance to cancel or reactivate.
                            self.main_processors
                                .borrow()
                                .iter()
                                .filter(|p| p.instance_id() != &e.instance_id)
                                .for_each(|p| {
                                    p.handle_change_of_some_upper_floor_instance(feedback_output)
                                });
                        }
                    }
                }
            }
        }
    }

    fn detect_reaper_config_changes(&mut self) {
        let changes = self.reaper_config_change_detector.poll_for_changes();
        for p in &*self.main_processors.borrow() {
            p.process_reaper_config_changes(&changes);
        }
    }

    fn emit_beats_as_feedback_events(&mut self) {
        for project in Reaper::get().projects() {
            let reference_pos = if project.is_playing() {
                project.play_position_latency_compensated()
            } else {
                project.edit_cursor_position()
            };
            if self.record_possible_beat_change(project, reference_pos) {
                let event = AdditionalFeedbackEvent::BeatChanged(BeatChangedEvent {
                    project,
                    new_value: reference_pos,
                });
                for p in &*self.main_processors.borrow() {
                    p.process_additional_feedback_event(&event);
                }
            }
        }
    }

    fn emit_device_changes_as_reaper_source_messages(&mut self, timestamp: ControlEventTimestamp) {
        // Check roughly every 2 seconds
        if self.counter % (30 * 2) == 0 {
            let midi_in_diff = self
                .device_change_detector
                .poll_for_midi_input_device_changes();
            let midi_out_diff = self
                .device_change_detector
                .poll_for_midi_output_device_changes();
            // Resetting MIDI devices is necessary especially on Windows.
            reset_midi_devices(
                midi_in_diff.added_devices.iter().copied(),
                midi_out_diff.added_devices.iter().copied(),
            );
            let mut msgs = Vec::with_capacity(2);
            if !midi_in_diff.added_devices.is_empty() || !midi_out_diff.added_devices.is_empty() {
                let payload = MidiDeviceChangePayload {
                    input_devices: midi_in_diff.added_devices,
                    output_devices: midi_out_diff.added_devices,
                };
                msgs.push(ReaperMessage::MidiDevicesConnected(payload));
            }
            if !midi_in_diff.removed_devices.is_empty() || !midi_out_diff.removed_devices.is_empty()
            {
                let payload = MidiDeviceChangePayload {
                    input_devices: midi_in_diff.removed_devices,
                    output_devices: midi_out_diff.removed_devices,
                };
                msgs.push(ReaperMessage::MidiDevicesDisconnected(payload));
            }
            for p in &mut *self.main_processors.borrow_mut() {
                for msg in &msgs {
                    let evt = ControlEvent::new(msg, timestamp);
                    p.process_reaper_message(evt);
                }
            }
        }
    }

    fn log_debug_info(&self) {
        // Summary
        let msg = format!(
            "\n\
            # Backbone control surface\n\
            \n\
            - Garbage count: {} \n\
            ",
            self.garbage_receiver.len(),
        );
        Reaper::get().show_console_msg(msg);
    }

    fn process_incoming_osc_messages(&mut self, timestamp: ControlEventTimestamp) {
        pub type PacketVec = SmallVec<[OscPacket; OSC_INCOMING_BULK_SIZE]>;
        let packets_by_device: SmallVec<[(OscDeviceId, PacketVec); OSC_INCOMING_BULK_SIZE]> = self
            .osc_input_devices
            .iter_mut()
            .map(|dev| {
                (
                    *dev.id(),
                    dev.poll_multiple(OSC_INCOMING_BULK_SIZE).collect(),
                )
            })
            .collect();
        for (dev_id, packets) in packets_by_device {
            match &self.state {
                State::Normal => {
                    for proc in &mut *self.main_processors.borrow_mut() {
                        if proc.wants_osc_from(&dev_id) {
                            for packet in &packets {
                                let evt = ControlEvent::new(packet, timestamp);
                                proc.process_incoming_osc_packet(evt);
                            }
                        }
                    }
                }
                State::CapturingOsc(sender) => {
                    for packet in packets {
                        process_incoming_osc_packet_for_learning(dev_id, sender, packet)
                    }
                }
                State::LearningTarget(_) => {}
            }
        }
    }

    fn handle_event_internal(
        &self,
        event: &ControlSurfaceEvent,
        change_event_queue: &mut Vec<ChangeEvent>,
    ) -> bool {
        // We always need to forward to the change detection middleware even if we are in
        // a mode in which the detected change event doesn't matter!
        self.change_detection_middleware.process(event, |e| {
            match &self.state {
                State::Normal => {
                    // We don't process change events immediately in order to be able to process
                    // multiple events occurring in one main loop cycle as a natural batch. This
                    // is important for performance reasons
                    // (see https://github.com/helgoboss/realearn/issues/553).
                    change_event_queue.push(e);
                }
                State::LearningTarget(sender) => {
                    // At some point we want the Rx stuff out of the domain layer. This is one step
                    // in this direction.
                    if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                        let _ = sender.try_send(target);
                    }
                }
                State::CapturingOsc(_) => {}
            }
        })
    }

    fn record_possible_beat_change(
        &mut self,
        project: Project,
        reference_pos: PositionInSeconds,
    ) -> bool {
        let beat_info = project.beat_info_at(reference_pos);
        let new_full_beats = beat_info.full_beats.get() as _;
        let full_beats = self.full_beats.entry(project.raw()).or_default();
        let beat_changed = new_full_beats != *full_beats;
        *full_beats = new_full_beats;
        beat_changed
    }
}

impl<EH: DomainEventHandler> ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware<EH> {
    fn run(&mut self) {
        let timestamp = ControlEventTimestamp::now();
        self.process_change_events();
        self.main_task_middleware.run();
        self.future_middleware.run();
        self.rx_middleware.run();
        self.process_main_tasks();
        self.process_incoming_additional_feedback();
        self.process_instance_orchestration_events();
        self.detect_reaper_config_changes();
        self.emit_beats_as_feedback_events();
        self.emit_device_changes_as_reaper_source_messages(timestamp);
        self.process_incoming_osc_messages(timestamp);
        self.poll_clip_matrixes();
        self.process_incoming_clip_matrix_events();
        self.run_main_processors(timestamp);
        self.drop_garbage();
        self.process_deferred_control_surface_events();
        self.counter += 1;
    }

    fn handle_event(&self, event: ControlSurfaceEvent) -> bool {
        // Reentrancy check (check if we are currently mutably in `run()`)
        // TODO-high We should do this in reaper-medium (in a more generic way) as soon as it turns
        //  out to work nicely. Related to this: https://github.com/helgoboss/reaper-rs/issues/54
        match self.change_event_queue.try_borrow_mut() {
            Ok(mut queue) => self.handle_event_internal(&event, &mut queue),
            Err(_) => {
                self.control_surface_event_sender
                    .send_complaining(event.clone().into_owned());
                false
            }
        }
    }

    fn get_touch_state(&self, args: GetTouchStateArgs) -> bool {
        if let Ok(domain_type) = TouchedTrackParameterType::try_from_reaper(args.parameter_type) {
            BackboneState::target_state()
                .borrow()
                .automation_parameter_is_touched(args.track, domain_type)
        } else {
            false
        }
    }

    fn ext_supports_extended_touch(&self, _: ExtSupportsExtendedTouchArgs) -> i32 {
        1
    }
}

fn process_incoming_osc_packet_for_learning(
    dev_id: OscDeviceId,
    sender: &OscCaptureSender,
    packet: OscPacket,
) {
    match packet {
        OscPacket::Message(msg) => process_incoming_osc_message_for_learning(dev_id, sender, msg),
        OscPacket::Bundle(bundle) => {
            for p in bundle.content.into_iter() {
                process_incoming_osc_packet_for_learning(dev_id, sender, p);
            }
        }
    }
}

fn process_incoming_osc_message_for_learning(
    dev_id: OscDeviceId,
    sender: &OscCaptureSender,
    message: OscMessage,
) {
    let scan_result = OscScanResult {
        message,
        dev_id: Some(dev_id),
    };
    let _ = sender.try_send(scan_result);
}

impl<EH: DomainEventHandler> Drop for RealearnControlSurfaceMiddleware<EH> {
    fn drop(&mut self) {
        for garbage in self.garbage_receiver.try_iter() {
            let _ = garbage;
        }
    }
}

/// For pushing deallocation to main thread (vs. doing it in the audio thread).
#[derive(Clone, Debug)]
pub struct GarbageBin {
    sender: SenderToNormalThread<Garbage>,
}
impl GarbageBin {
    pub fn new(sender: SenderToNormalThread<Garbage>) -> Self {
        assert!(
            sender.is_bounded(),
            "garbage bin sender channel must be bounded!"
        );
        Self { sender }
    }

    /// Pushes deallocation to the main thread.
    pub fn dispose(&self, garbage: Garbage) {
        self.sender.send_complaining(garbage);
    }

    pub fn dispose_real_time_mapping(&self, m: RealTimeMapping) {
        // Dispose bits that contain heap-allocated stuff. Do it separately to not let the garbage
        // enum size get too large.
        self.dispose(Garbage::LifecycleMidiData(m.lifecycle_midi_data));
        self.dispose(Garbage::ResolvedTarget(m.resolved_target));
        let mode_garbage = m.core.mode.recycle();
        self.dispose(Garbage::Mode(mode_garbage));
        self.dispose(Garbage::MappingSource(m.core.source));
    }
}

fn reset_midi_devices(
    in_devs: impl Iterator<Item = MidiInputDeviceId>,
    out_devs: impl Iterator<Item = MidiOutputDeviceId>,
) {
    let reaper_low = Reaper::get().medium_reaper().low();
    if reaper_low.pointers().midi_init.is_none() {
        // REAPER version < 6.47
        return;
    }
    for res in in_devs.zip_longest(out_devs) {
        let (input_arg, output_arg) = match res {
            EitherOrBoth::Both(i, o) => (i.get() as i32, o.get() as i32),
            EitherOrBoth::Left(i) => (i.get() as i32, -1),
            EitherOrBoth::Right(o) => (-1, o.get() as i32),
        };
        reaper_low.midi_init(input_arg, output_arg);
    }
}
