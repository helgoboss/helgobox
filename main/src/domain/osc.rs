use derive_more::Display;
use rosc::OscPacket;
use serde::{Deserialize, Serialize};

use slog::warn;

use std::error::Error;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, ToSocketAddrs, UdpSocket};

use uuid::Uuid;

const OSC_BUFFER_SIZE: usize = 10_000;

#[derive(Debug)]
pub struct OscInputDevice {
    id: OscDeviceId,
    socket: UdpSocket,
    logger: slog::Logger,
    osc_buffer: [u8; OSC_BUFFER_SIZE],
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
            osc_buffer: [0; OSC_BUFFER_SIZE],
        };
        Ok(dev)
    }

    pub fn id(&self) -> &OscDeviceId {
        &self.id
    }

    pub fn poll(&mut self) -> Result<Option<OscPacket>, &'static str> {
        match self.socket.recv(&mut self.osc_buffer) {
            Ok(num_bytes) => match rosc::decoder::decode(&self.osc_buffer[..num_bytes]) {
                Ok(packet) => Ok(Some(packet)),
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
}

impl OscOutputDevice {
    pub fn connect(
        id: OscDeviceId,
        addr: impl ToSocketAddrs,
        logger: slog::Logger,
    ) -> Result<OscOutputDevice, Box<dyn Error>> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        socket.connect(addr)?;
        let dev = OscOutputDevice { id, socket, logger };
        Ok(dev)
    }

    pub fn id(&self) -> &OscDeviceId {
        &self.id
    }

    pub fn send(&self, packet: &OscPacket) -> Result<(), &'static str> {
        let bytes =
            rosc::encoder::encode(packet).map_err(|_| "error trying to encode OSC packet")?;
        self.socket
            .send(&bytes)
            .map_err(|_| "error trying to send OSC packet")?;
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
}
