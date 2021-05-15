use crossbeam_channel::Receiver;
use derive_more::Display;
use rosc::{OscBundle, OscMessage, OscPacket};
use serde::{Deserialize, Serialize};

use slog::{trace, warn};

use std::error::Error;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, ToSocketAddrs, UdpSocket};

use core::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use uuid::Uuid;

const MAX_INCOMING_PACKET_SIZE: usize = 10_000;
const OSC_OUTGOING_BULK_SIZE: usize = 16;

pub struct OscFeedbackTask {
    dev_id: OscDeviceId,
    msg: OscMessage,
}

impl OscFeedbackTask {
    pub fn new(dev_id: OscDeviceId, msg: OscMessage) -> Self {
        Self { dev_id, msg }
    }
}

#[derive(Debug)]
pub struct OscFeedbackProcessor {
    state: State,
}

#[derive(Debug)]
enum State {
    Stopped(StoppedState),
    Starting,
    Running(RunningState),
    Stopping,
}

#[derive(Debug)]
struct StoppedState {
    task_receiver: Receiver<OscFeedbackTask>,
}

#[derive(Debug)]
struct RunningState {
    request_stop: Arc<AtomicBool>,
    join_handle: JoinHandle<OscFeedbackHandler>,
}

impl OscFeedbackProcessor {
    pub fn new(task_receiver: Receiver<OscFeedbackTask>) -> Self {
        Self {
            state: State::Stopped(StoppedState { task_receiver }),
        }
    }

    pub fn start(&mut self, osc_output_devices: Vec<OscOutputDevice>) {
        if osc_output_devices.is_empty() || !matches!(&self.state, State::Stopped(_)) {
            return;
        }
        let state = if let State::Stopped(s) = mem::replace(&mut self.state, State::Starting) {
            s
        } else {
            panic!("manager was not stopped");
        };
        let mut handler = OscFeedbackHandler {
            task_receiver: state.task_receiver,
            osc_output_devices,
        };
        let request_stop = Arc::new(AtomicBool::new(false));
        let request_stop_clone = request_stop.clone();
        let join_handle = std::thread::Builder::new()
            .name("ReaLearn OSC sender".to_owned())
            .spawn(move || {
                while !request_stop_clone.load(Ordering::SeqCst) {
                    handler.cycle();
                }
                handler
            })
            .unwrap();
        self.state = State::Running(RunningState {
            request_stop,
            join_handle,
        });
    }

    pub fn stop(&mut self) {
        if !matches!(&self.state, State::Running(_)) {
            return;
        }
        let state = if let State::Running(s) = mem::replace(&mut self.state, State::Stopping) {
            s
        } else {
            panic!("manager was not started");
        };
        state.request_stop.store(true, Ordering::SeqCst);
        let handler = state.join_handle.join().unwrap();
        self.state = State::Stopped(StoppedState {
            task_receiver: handler.return_task_receiver(),
        });
    }
}

struct OscFeedbackHandler {
    task_receiver: Receiver<OscFeedbackTask>,
    osc_output_devices: Vec<OscOutputDevice>,
}

impl OscFeedbackHandler {
    pub fn cycle(&mut self) {
        use itertools::Itertools;
        let grouped_by_device = self
            .task_receiver
            .try_iter()
            .take(OSC_OUTGOING_BULK_SIZE)
            .sorted_by_key(|task| task.dev_id)
            .group_by(|task| task.dev_id);
        for (dev_id, group) in grouped_by_device.into_iter() {
            if let Some(dev) = self.osc_output_devices.iter().find(|d| d.id() == dev_id) {
                let _ = dev.send(group.map(|task| task.msg));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    pub fn return_task_receiver(self) -> Receiver<OscFeedbackTask> {
        self.task_receiver
    }
}

#[derive(Debug)]
pub struct OscInputDevice {
    id: OscDeviceId,
    socket: UdpSocket,
    logger: slog::Logger,
    osc_buffer: [u8; MAX_INCOMING_PACKET_SIZE],
}

impl OscInputDevice {
    pub fn bind(
        id: OscDeviceId,
        addr: impl ToSocketAddrs,
        logger: slog::Logger,
    ) -> Result<OscInputDevice, Box<dyn Error>> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        let dev = OscInputDevice {
            id,
            socket,
            logger,
            osc_buffer: [0; MAX_INCOMING_PACKET_SIZE],
        };
        Ok(dev)
    }

    pub fn id(&self) -> &OscDeviceId {
        &self.id
    }

    pub fn poll(&mut self) -> Result<Option<OscPacket>, &'static str> {
        match self.socket.recv(&mut self.osc_buffer) {
            Ok(num_bytes) => match rosc::decoder::decode(&self.osc_buffer[..num_bytes]) {
                Ok(packet) => {
                    trace!(
                        self.logger,
                        "Received packet with {} bytes: {:#?}",
                        num_bytes,
                        &packet
                    );
                    Ok(Some(packet))
                }
                Err(err) => {
                    warn!(self.logger, "Error trying to decode OSC packet: {:?}", err);
                    Err("error trying to decode OSC messages")
                }
            },
            Err(ref err) if err.kind() != io::ErrorKind::WouldBlock => {
                warn!(self.logger, "Error trying to receive OSC packet: {}", err);
                Err("error trying to receive OSC message")
            }
            // We don't need to handle "would block" because we are running in a loop anyway.
            _ => Ok(None),
        }
    }

    pub fn poll_multiple(&mut self, n: usize) -> impl Iterator<Item = OscPacket> + '_ {
        (0..n).flat_map(move |_| self.poll().ok().flatten())
    }
}

#[derive(Debug)]
pub struct OscOutputDevice {
    id: OscDeviceId,
    socket: UdpSocket,
    logger: slog::Logger,
    can_deal_with_bundles: bool,
}

impl OscOutputDevice {
    pub fn connect(
        id: OscDeviceId,
        addr: impl ToSocketAddrs,
        logger: slog::Logger,
        can_deal_with_bundles: bool,
    ) -> Result<OscOutputDevice, Box<dyn Error>> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.connect(addr)?;
        let dev = OscOutputDevice {
            id,
            socket,
            logger,
            can_deal_with_bundles,
        };
        Ok(dev)
    }

    pub fn id(&self) -> OscDeviceId {
        self.id
    }

    pub fn send(&self, messages: impl Iterator<Item = OscMessage>) -> Result<(), &'static str> {
        if self.can_deal_with_bundles {
            // Haven't realized a performance difference between sending a bundle or single
            // messages. However, REAPER sends a bundle (maybe in order to use time tags).
            // Let's do it, too, if the device supports it.
            self.send_as_bundle(messages)
        } else {
            self.send_as_messages(messages)
        }
    }

    fn send_as_bundle(
        &self,
        messages: impl Iterator<Item = OscMessage>,
    ) -> Result<(), &'static str> {
        let bundle = OscBundle {
            // That should be "immediately" according to the OSC Time Tag spec.
            timetag: (0, 1).into(),
            content: messages.map(OscPacket::Message).collect(),
        };
        let packet = OscPacket::Bundle(bundle);
        let bytes = rosc::encoder::encode(&packet)
            .map_err(|_| "error trying to encode OSC bundle packet")?;
        trace!(
            self.logger,
            "Sending bundle packet with {} bytes: {:#?}",
            bytes.len(),
            &packet
        );
        self.socket
            .send(&bytes)
            .map_err(|_| "error trying to send OSC bundle packet")?;
        Ok(())
    }

    fn send_as_messages(
        &self,
        messages: impl Iterator<Item = OscMessage>,
    ) -> Result<(), &'static str> {
        for m in messages {
            let packet = OscPacket::Message(m);
            let bytes = rosc::encoder::encode(&packet)
                .map_err(|_| "error trying to encode OSC message packet")?;
            trace!(
                self.logger,
                "Sending message packet with {} bytes: {:#?}",
                bytes.len(),
                &packet
            );
            self.socket
                .send(&bytes)
                .map_err(|_| "error trying to send OSC message packet")?;
        }
        Ok(())
    }
}

/// An OSC device ID.
///
/// This uniquely identifies an OSC device according to ReaLearn's device configuration.
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Display, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct OscDeviceId(uuid::Uuid);

impl OscDeviceId {
    pub fn random() -> OscDeviceId {
        OscDeviceId(Uuid::new_v4())
    }

    pub fn fmt_short(&self) -> String {
        self.0.to_string().chars().take(5).collect()
    }
}
