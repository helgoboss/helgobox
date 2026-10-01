use crate::domain::UnitId;
use anyhow::Context;
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use elgato_streamdeck::info::Kind;
use elgato_streamdeck::{StreamDeck, list_devices, refresh_device_list};
use hidapi::{HidApi, HidResult};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::sync::{LazyLock, Mutex, MutexGuard};

static HID_API: LazyLock<HidResult<Mutex<HidApi>>> =
    LazyLock::new(|| HidApi::new().map(Mutex::new));

fn hid_api() -> anyhow::Result<MutexGuard<'static, HidApi>> {
    match &*HID_API {
        Ok(api) => Ok(api.lock().unwrap()),
        Err(e) => Err(e.into()),
    }
}

pub struct ProbedStreamDeckDevice {
    pub dev: StreamDeckDevice,
}

#[derive(Clone, Debug)]
pub struct StreamDeckDevice {
    pub id: StreamDeckDeviceId,
    pub name: String,
}

impl StreamDeckDevice {
    pub const fn new(id: StreamDeckDeviceId, name: String) -> Self {
        Self { id, name }
    }
}

pub fn probe_stream_deck_devices() -> anyhow::Result<Vec<ProbedStreamDeckDevice>> {
    let mut hid_api = hid_api()?;
    refresh_device_list(&mut hid_api)?;
    let devices = list_devices(&hid_api)
        .into_iter()
        .map(|(kind, serial)| ProbedStreamDeckDevice {
            dev: StreamDeckDevice::new(
                StreamDeckDeviceId::from_kind_and_serial(kind, Some(serial.clone())),
                format!("{kind:?} ({serial})"),
            ),
        })
        .collect();
    Ok(devices)
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct StreamDeckDeviceId {
    /// Vendor ID.
    pub vid: u16,
    /// Product ID.
    pub pid: u16,
    // Serial number (for distinguishing between multiple devices of the same type).
    pub serial: Option<String>,
}

impl Display for StreamDeckDeviceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.vid, self.pid)
    }
}

impl StreamDeckDeviceId {
    pub fn from_kind_and_serial(kind: Kind, serial: Option<String>) -> Self {
        Self {
            vid: kind.vendor_id(),
            pid: kind.product_id(),
            serial,
        }
    }

    pub fn connect(&self) -> anyhow::Result<StreamDeck> {
        let hid_api = hid_api()?;
        let desired_kind = self.kind().context("unknown kind of StreamDeck")?;
        let serial: Cow<str> = if let Some(serial) = &self.serial {
            serial.into()
        } else {
            let (_, serial) = list_devices(&hid_api)
                .into_iter()
                .find(|(kind, _)| *kind == desired_kind)
                .context("not StreamDeck of that kind connected")?;
            serial.into()
        };
        let sd = StreamDeck::connect(&hid_api, desired_kind, &serial)?;
        Ok(sd)
    }

    pub fn kind(&self) -> Option<Kind> {
        Kind::from_vid_pid(self.vid, self.pid)
    }
}

#[derive(Debug, Default)]
pub struct StreamDeckDeviceManager {
    device_usage: NonCryptoHashMap<UnitId, StreamDeckDeviceId>,
}

impl StreamDeckDeviceManager {
    pub fn register_device_usage(&mut self, unit_id: UnitId, device: Option<StreamDeckDeviceId>) {
        if let Some(d) = device {
            self.device_usage.insert(unit_id, d);
        } else {
            self.device_usage.remove(&unit_id);
        }
    }

    pub fn devices_in_use(&self) -> NonCryptoHashSet<&StreamDeckDeviceId> {
        self.device_usage.values().collect()
    }
}
