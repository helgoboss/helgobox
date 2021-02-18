use crate::core::Global;
use crate::domain::{
    DomainEventHandler, DomainGlobal, MainProcessor, OscDeviceId, OscInputDevice, OscPacketVec,
    ReaperTarget,
};
use crossbeam_channel::Receiver;
use helgoboss_learn::{OscSource, OscSourceValue};
use reaper_high::{
    ChangeDetectionMiddleware, ControlSurfaceEvent, ControlSurfaceMiddleware, FutureMiddleware,
    MainTaskMiddleware, MeterMiddleware,
};
use reaper_rx::ControlSurfaceRxMiddleware;
use rosc::{OscError, OscMessage, OscPacket};
use slog::warn;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::io;
use std::io::Error;
use std::net::{SocketAddr, UdpSocket};

type LearnSourceSender = async_channel::Sender<(OscDeviceId, OscSource)>;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    logger: slog::Logger,
    change_detection_middleware: ChangeDetectionMiddleware,
    rx_middleware: ControlSurfaceRxMiddleware,
    main_processors: Vec<MainProcessor<EH>>,
    main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
    server_task_receiver: Receiver<RealearnControlSurfaceServerTask>,
    meter_middleware: MeterMiddleware,
    main_task_middleware: MainTaskMiddleware,
    future_middleware: FutureMiddleware,
    counter: u64,
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
    // If I don't box this, I get a stack overflow. WTH! Maybe related to smallvec?
    AddOscDevice(Box<OscInputDevice>),
    RemoveOscDevice(OscDeviceId),
    LogDebugInfo,
    StartLearningTargets(async_channel::Sender<ReaperTarget>),
    StartLearningSources(LearnSourceSender),
    StopLearning,
}

pub enum RealearnControlSurfaceServerTask {
    ProvidePrometheusMetrics(tokio::sync::oneshot::Sender<String>),
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    pub fn new(
        parent_logger: &slog::Logger,
        main_task_receiver: Receiver<RealearnControlSurfaceMainTask<EH>>,
        server_task_receiver: Receiver<RealearnControlSurfaceServerTask>,
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
            metrics_enabled,
            state: State::Normal,
            osc_input_devices: vec![],
        }
    }

    pub fn remove_main_processor(&mut self, id: &str) {
        self.main_processors.retain(|p| p.instance_id() != id);
    }

    pub fn reset(&self) {
        self.change_detection_middleware.reset(|e| {
            self.rx_middleware.handle_change(e);
        });
        // We don't want to execute tasks which accumulated during the "downtime" of Reaper.
        // So we just consume all without executing them.
        self.main_task_middleware.reset();
        self.future_middleware.reset();
    }

    pub fn set_osc_devices(&mut self, devices: Vec<OscInputDevice>) {
        self.osc_input_devices = devices;
    }

    pub fn clear_osc_devices(&mut self) {
        self.osc_input_devices.clear();
    }

    fn run_internal(&mut self) {
        self.main_task_middleware.run();
        self.future_middleware.run();
        self.rx_middleware.run();
        for t in self.main_task_receiver.try_iter().take(10) {
            use RealearnControlSurfaceMainTask::*;
            match t {
                AddMainProcessor(p) => {
                    self.main_processors.push(p);
                }
                AddOscDevice(dev) => {
                    self.osc_input_devices.push(*dev);
                }
                RemoveOscDevice(dev_id) => {
                    self.osc_input_devices.retain(|dev| dev.id() != &dev_id);
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
            }
        }
        for t in self.server_task_receiver.try_iter().take(10) {
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
        self.process_osc();
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

    fn process_osc(&mut self) {
        let osc_packets_by_device: SmallVec<[(OscDeviceId, OscPacketVec); 32]> = self
            .osc_input_devices
            .iter_mut()
            .map(|dev| (dev.id().clone(), dev.poll_multiple()))
            .collect();
        for (dev_id, packets) in osc_packets_by_device {
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
                        process_incoming_osc_packet_for_learning(&dev_id, sender, packet)
                    }
                }
                State::LearningTarget(_) => {}
            }
        }
    }

    fn handle_event_internal(&self, event: ControlSurfaceEvent) {
        // We always need to forward to the change detection middleware even if we are in
        // a mode in which the detected change event doesn't matter!
        self.change_detection_middleware.process(event, |e| {
            match &self.state {
                State::Normal => {
                    self.rx_middleware.handle_change(e.clone());
                    if let Some(target) = ReaperTarget::touched_from_change_event(e) {
                        DomainGlobal::get().set_last_touched_target(target);
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
        });
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

    fn handle_event(&self, event: ControlSurfaceEvent) {
        if self.metrics_enabled {
            let elapsed = MeterMiddleware::measure(|| {
                self.handle_event_internal(event);
            });
            self.meter_middleware.record_event(event, elapsed);
        } else {
            self.handle_event_internal(event);
        }
    }
}

fn process_incoming_osc_packet_for_learning(
    dev_id: &OscDeviceId,
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
    dev_id: &OscDeviceId,
    sender: &LearnSourceSender,
    msg: OscMessage,
) {
    let value = OscSourceValue::Plain(&msg);
    if let Some(source) = OscSource::from_source_value(value) {
        let _ = sender.try_send((dev_id.clone(), source));
    }
}
