use derive_more::Display;
use rosc::OscPacket;
use serde::{Deserialize, Serialize};
use serde_with::DeserializeFromStr;
use slog::warn;
use smallvec::SmallVec;
use std::error::Error;
use std::io;
use std::net::{ToSocketAddrs, UdpSocket};
use std::str::FromStr;

const OSC_BULK_SIZE: usize = 32;
const OSC_BUFFER_SIZE: usize = 10_000;
pub type OscPacketVec = SmallVec<[OscPacket; OSC_BULK_SIZE]>;

#[derive(Debug)]
pub struct OscInputDevice {
    id: OscDeviceId,
    input_socket: UdpSocket,
    logger: slog::Logger,
    osc_buffer: [u8; OSC_BUFFER_SIZE],
}

impl OscInputDevice {
    pub fn connect(
        id: OscDeviceId,
        addr: impl ToSocketAddrs,
        logger: slog::Logger,
    ) -> Result<OscInputDevice, Box<dyn Error>> {
        let s = UdpSocket::bind(addr).unwrap();
        s.set_nonblocking(true)?;
        let dev = OscInputDevice {
            id,
            input_socket: s,
            logger,
            osc_buffer: [0; OSC_BUFFER_SIZE],
        };
        Ok(dev)
    }

    pub fn id(&self) -> &OscDeviceId {
        &self.id
    }

    pub fn poll(&mut self) -> Result<Option<OscPacket>, &'static str> {
        match self.input_socket.recv(&mut self.osc_buffer) {
            Ok(num_bytes) => match rosc::decoder::decode(&self.osc_buffer[..num_bytes]) {
                Ok(packet) => Ok(Some(packet)),
                Err(err) => {
                    warn!(self.logger, "Error trying to decode OSC message: {:?}", err);
                    Err("error trying to decode OSC messages")
                }
            },
            Err(ref err) if err.kind() != io::ErrorKind::WouldBlock => {
                warn!(self.logger, "Error trying to receive OSC message: {}", err);
                Err("error trying to receive OSC message")
            }
            // We don't need to handle "would block" because we are running in a loop anyway.
            _ => Ok(None),
        }
    }

    pub fn poll_multiple(&mut self) -> OscPacketVec {
        (0..OSC_BULK_SIZE)
            .flat_map(|_| self.poll().ok().flatten())
            .collect()
    }
}

/// An OSC device ID.
///
/// This uniquely identifies an OSC device according to ReaLearn's device configuration.
#[derive(
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    Default,
    Display,
    Serialize,
    DeserializeFromStr,
)]
pub struct OscDeviceId(String);

impl FromStr for OscDeviceId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("OSC device ID must not be empty");
        }
        let valid_regex = regex!(r#"^[A-Za-z0-9_~]$"#);
        if valid_regex.is_match(trimmed) {
            return Err("OSC device must contain lowercase letters, digits and hyphens only");
        }
        Ok(OscDeviceId(trimmed.to_owned()))
    }
}
