use crate::core::Global;
use crate::domain::{
    BackboneState, ControlInput, DeviceControlInput, DeviceFeedbackOutput, DomainEventHandler,
    FeedbackOutput, MainProcessor, OscDeviceId, OscInputDevice, RealSource, ReaperTarget,
    SourceFeedbackValue, TouchedParameterType,
};
use crossbeam_channel::Receiver;
use helgoboss_learn::OscSource;
use reaper_high::{
    ChangeDetectionMiddleware, ControlSurfaceEvent, ControlSurfaceMiddleware, FutureMiddleware, Fx,
    FxParameter, MainTaskMiddleware, MeterMiddleware, Project, Reaper,
};
use reaper_rx::ControlSurfaceRxMiddleware;
use rosc::{OscMessage, OscPacket};

use reaper_medium::{
    CommandId, ExtSupportsExtendedTouchArgs, GetTouchStateArgs, MediaTrack, PositionInSeconds,
    ReaperNormalizedFxParamValue,
};
use rxrust::prelude::*;
use smallvec::SmallVec;
use std::collections::HashMap;

type LearnSourceSender = async_channel::Sender<(OscDeviceId, OscSource)>;

const CONTROL_SURFACE_MAIN_TASK_BULK_SIZE: usize = 10;
const CONTROL_SURFACE_SERVER_TASK_BULK_SIZE: usize = 10;
const ADDITIONAL_FEEDBACK_EVENT_BULK_SIZE: usize = 30;
const INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE: usize = 30;
const OSC_INCOMING_BULK_SIZE: usize = 32;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    logger: slog::Logger,
    change_detection_middleware: ChangeDetectionMiddleware,
    rx_middleware: ControlSurfaceRxMiddleware,
    main_processors: Vec<MainProcessor<EH>>,
    main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
    server_task_receiver: Receiver<RealearnControlSurfaceServerTask>,
    additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
    instance_orchestration_event_receiver: Receiver<InstanceOrchestrationEvent>,
    meter_middleware: MeterMiddleware,
    main_task_middleware: MainTaskMiddleware,
    future_middleware: FutureMiddleware,
    counter: u64,
    full_beats: u32,
    metrics_enabled: bool,
    state: State,
    osc_input_devices: Vec<OscInputDevice>,
}

#[derive(Debug)]
enum State {
    Normal,
    LearningSource(LearnSourceSender),
    LearningTarget(async_channel::Sender<ReaperTarget>),
}

pub enum RealearnControlSurfaceMainTask<EH: DomainEventHandler> {
    // Removing a main processor is done synchronously by temporarily regaining ownership of the
    // control surface from REAPER.
    AddMainProcessor(MainProcessor<EH>),
    LogDebugInfo,
    StartLearningTargets(async_channel::Sender<ReaperTarget>),
    StartLearningSources(LearnSourceSender),
    StopLearning,
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
    PlayPositionChanged(PlayPositionChangedEvent),
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
    pub instance_id: String,
    pub control_input: Option<DeviceControlInput>,
    pub control_input_used: bool,
    pub control_input_usage_might_have_changed: bool,
    pub feedback_output: Option<DeviceFeedbackOutput>,
    pub feedback_output_used: bool,
    pub feedback_output_usage_might_have_changed: bool,
}

#[derive(Debug)]
pub struct SourceReleasedEvent {
    pub instance_id: String,
    pub feedback_output: FeedbackOutput,
    pub feedback_value: SourceFeedbackValue,
}

#[derive(Debug)]
pub struct PlayPositionChangedEvent {
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
    pub parameter_type: TouchedParameterType,
    pub new_value: bool,
}

pub enum RealearnControlSurfaceServerTask {
    ProvidePrometheusMetrics(tokio::sync::oneshot::Sender<String>),
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    pub fn new(
        parent_logger: &slog::Logger,
        main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
        server_task_receiver: Receiver<RealearnControlSurfaceServerTask>,
        additional_feedback_event_receiver: Receiver<AdditionalFeedbackEvent>,
        instance_orchestration_event_receiver: Receiver<InstanceOrchestrationEvent>,
        metrics_enabled: bool,
    ) -> Self {
        let logger = parent_logger.new(slog::o!("struct" => "RealearnControlSurfaceMiddleware"));
        Self {
            logger: logger.clone(),
            change_detection_middleware: ChangeDetectionMiddleware::new(),
            rx_middleware: ControlSurfaceRxMiddleware::new(Global::control_surface_rx().clone()),
            main_processors: Default::default(),
            main_task_receiver,
            server_task_receiver,
            additional_feedback_event_receiver,
            instance_orchestration_event_receiver,
            meter_middleware: MeterMiddleware::new(logger.clone()),
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
            full_beats: 0,
            metrics_enabled,
            state: State::Normal,
            osc_input_devices: vec![],
        }
    }

    pub fn remove_main_processor(&mut self, id: &str) {
        self.main_processors.retain(|p| p.instance_id() != id);
    }

    pub fn set_osc_input_devices(&mut self, devs: Vec<OscInputDevice>) {
        self.osc_input_devices = devs;
    }

    pub fn clear_osc_input_devices(&mut self) {
        self.osc_input_devices.clear();
    }

    /// Called when waking up ReaLearn (first instance appears again or the first time).
    pub fn wake_up(&self) {
        self.change_detection_middleware.reset(|e| {
            for m in &self.main_processors {
                m.process_control_surface_change_event(&e);
            }
            self.rx_middleware.handle_change(e);
        });
        // We don't want to execute tasks which accumulated during the "downtime" of Reaper.
        // So we just consume all without executing them.
        self.main_task_middleware.reset();
        self.future_middleware.reset();
    }

    fn run_internal(&mut self) {
        // Run middlewares
        self.main_task_middleware.run();
        self.future_middleware.run();
        self.rx_middleware.run();
        // Process main tasks
        for t in self
            .main_task_receiver
            .try_iter()
            .take(CONTROL_SURFACE_MAIN_TASK_BULK_SIZE)
        {
            use RealearnControlSurfaceMainTask::*;
            match t {
                AddMainProcessor(p) => {
                    self.main_processors.push(p);
                }
                LogDebugInfo => {
                    self.meter_middleware.log_metrics();
                }
                StartLearningTargets(sender) => {
                    self.state = State::LearningTarget(sender);
                }
                StopLearning => {
                    self.state = State::Normal;
                }
                StartLearningSources(sender) => {
                    self.state = State::LearningSource(sender);
                }
                SendAllFeedback => {
                    for m in &self.main_processors {
                        m.send_all_feedback();
                    }
                }
            }
        }
        // Process server tasks
        for t in self
            .server_task_receiver
            .try_iter()
            .take(CONTROL_SURFACE_SERVER_TASK_BULK_SIZE)
        {
            use RealearnControlSurfaceServerTask::*;
            match t {
                ProvidePrometheusMetrics(sender) => {
                    let text = serde_prometheus::to_string(
                        self.meter_middleware.metrics(),
                        Some("realearn"),
                        HashMap::new(),
                    )
                    .unwrap();
                    let _ = sender.send(text);
                }
            }
        }
        // Process incoming additional feedback
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
            for p in &mut self.main_processors {
                p.process_additional_feedback_event(&event)
            }
        }
        // Process instance orchestration events
        for event in self
            .instance_orchestration_event_receiver
            .try_iter()
            .take(INSTANCE_ORCHESTRATION_EVENT_BULK_SIZE)
        {
            use InstanceOrchestrationEvent::*;
            match event {
                SourceReleased(e) => {
                    println!("SOURCE RELEASED {}", e.instance_id);
                    let other_instance_took_over =
                        if let Some(source) = RealSource::from_feedback_value(&e.feedback_value) {
                            self.main_processors
                                .iter()
                                .filter(|p| p.instance_id() != &e.instance_id)
                                .any(|p| p.maybe_takeover_source(&source))
                        } else {
                            false
                        };
                    if !other_instance_took_over {
                        if let Some(p) = self
                            .main_processors
                            .iter()
                            .find(|p| p.instance_id() == &e.instance_id)
                        {
                            p.finally_switch_off_source(e.feedback_output, e.feedback_value);
                        }
                    }
                }
                IoUpdated(e) => {
                    let backbone_state = BackboneState::get();
                    backbone_state.update_io_usage(
                        e.instance_id.clone(),
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
                    if e.feedback_output_usage_might_have_changed
                        && backbone_state.lives_on_upper_floor(&e.instance_id)
                    {
                        if let Some(feedback_output) = e.feedback_output {
                            // Give lower-floor instances the chance to cancel or reactivate.
                            self.main_processors
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
        // Emit beats as feedback events
        // TODO-medium Make multi-project compatible.
        let project = Reaper::get().current_project();
        let reference_pos = if project.is_playing() {
            project.play_position_latency_compensated()
        } else {
            project.edit_cursor_position()
        };
        if self.beat_has_changed(project, reference_pos) {
            let event = AdditionalFeedbackEvent::PlayPositionChanged(PlayPositionChangedEvent {
                new_value: reference_pos,
            });
            for p in &mut self.main_processors {
                p.process_additional_feedback_event(&event);
            }
        }
        // OSC
        self.process_incoming_osc_messages();
        // Main processors
        match &self.state {
            State::Normal => {
                for p in &mut self.main_processors {
                    p.run_all();
                }
            }
            State::LearningSource(_) | State::LearningTarget(_) => {
                for p in &mut self.main_processors {
                    p.run_essential();
                }
            }
        }
        // Metrics
        if self.metrics_enabled {
            // Roughly every 10 seconds
            if self.counter == 30 * 10 {
                self.meter_middleware.warn_about_critical_metrics();
                self.counter = 0;
            } else {
                self.counter += 1;
            }
        }
    }

    fn process_incoming_osc_messages(&mut self) {
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
                    for proc in &mut self.main_processors {
                        if proc.receives_osc_from(&dev_id) {
                            for packet in &packets {
                                proc.process_incoming_osc_packet(packet);
                            }
                        }
                    }
                }
                State::LearningSource(sender) => {
                    for packet in packets {
                        process_incoming_osc_packet_for_learning(dev_id, sender, packet)
                    }
                }
                State::LearningTarget(_) => {}
            }
        }
    }

    fn handle_event_internal(&self, event: ControlSurfaceEvent) -> bool {
        // We always need to forward to the change detection middleware even if we are in
        // a mode in which the detected change event doesn't matter!
        self.change_detection_middleware.process(event, |e| {
            match &self.state {
                State::Normal => {
                    // This is for feedback processing. No Rx!
                    for m in &self.main_processors {
                        m.process_control_surface_change_event(&e);
                    }
                    // The rest is only for upper layers (e.g. UI), not for processing.
                    self.rx_middleware.handle_change(e.clone());
                    if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                        // TODO-medium Now we have the necessary framework (AdditionalFeedbackEvent)
                        //  to also support action, FX snapshot and ReaLearn monitoring FX parameter
                        //  touching for "Last touched" target and global learning (see
                        //  LearningTarget state)! Connect the dots!
                        BackboneState::get().set_last_touched_target(target);
                        for p in &self.main_processors {
                            p.notify_target_touched();
                        }
                    }
                }
                State::LearningTarget(sender) => {
                    // At some point we want the Rx stuff out of the domain layer. This is one step
                    // in this direction.
                    if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                        let _ = sender.try_send(target);
                    }
                }
                State::LearningSource(_) => {}
            }
        })
    }

    fn beat_has_changed(&mut self, project: Project, reference_pos: PositionInSeconds) -> bool {
        let beat_info = project.beat_info_at(reference_pos);
        let new_full_beats = beat_info.full_beats.get() as _;
        let beat_changed = new_full_beats != self.full_beats;
        self.full_beats = new_full_beats;
        beat_changed
    }
}

impl<EH: DomainEventHandler> ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware<EH> {
    fn run(&mut self) {
        if self.metrics_enabled {
            let elapsed = MeterMiddleware::measure(|| {
                self.run_internal();
            });
            self.meter_middleware.record_run(elapsed);
        } else {
            self.run_internal();
        }
    }

    fn handle_event(&self, event: ControlSurfaceEvent) -> bool {
        if self.metrics_enabled {
            let elapsed = MeterMiddleware::measure(|| {
                self.handle_event_internal(event);
            });
            self.meter_middleware.record_event(event, elapsed)
        } else {
            self.handle_event_internal(event)
        }
    }

    fn get_touch_state(&self, args: GetTouchStateArgs) -> bool {
        if let Ok(domain_type) = TouchedParameterType::try_from_reaper(args.parameter_type) {
            BackboneState::target_context()
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
    sender: &LearnSourceSender,
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
    sender: &LearnSourceSender,
    msg: OscMessage,
) {
    let source = OscSource::from_source_value(msg, Some(0));
    let _ = sender.try_send((dev_id, source));
}
