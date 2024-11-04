use crate::domain::UnitId;
use base::hash_util::{NonCryptoHashMap, NonCryptoHashSet};
use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use streamdeck::{pids, StreamDeck};

pub struct ProbedStreamDeckDevice {
    pub dev: StreamDeckDevice,
    pub available: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct StreamDeckDevice {
    pub id: StreamDeckDeviceId,
    pub name: &'static str,
}

impl StreamDeckDevice {
    pub const fn new(vid: u16, pid: u16, name: &'static str) -> Self {
        let id = StreamDeckDeviceId { vid, pid };
        Self { id, name }
    }
}

pub fn probe_stream_deck_devices() -> anyhow::Result<Vec<ProbedStreamDeckDevice>> {
    let mut api = HidApi::new()?;
    api.refresh_devices()?;
    let connected_devs: NonCryptoHashSet<_> = api
        .device_list()
        .map(|info| StreamDeckDeviceId {
            vid: info.vendor_id(),
            pid: info.product_id(),
        })
        .collect();
    let probed_devs = SUPPORTED_DEVICES
        .iter()
        .copied()
        .map(|dev| ProbedStreamDeckDevice {
            dev,
            available: connected_devs.contains(&dev.id),
        })
        .collect();
    Ok(probed_devs)
}

const ELGATO_VENDOR_ID: u16 = 0x0fd9;

const SUPPORTED_DEVICES: &[StreamDeckDevice] = &[
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::ORIGINAL, "Original"),
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::ORIGINAL_V2, "Original v2"),
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::MINI, "Mini"),
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::XL, "XL"),
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::MK2, "MK2"),
    StreamDeckDevice::new(ELGATO_VENDOR_ID, pids::REVISED_MINI, "Revised Mini"),
];

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct StreamDeckDeviceId {
    /// Vendor ID.
    pub vid: u16,
    /// Product ID.
    pub pid: u16,
    // Serial number (for distinguishing between multiple devices of the same type).
    // pub serial_number: Option<String>,
}

impl StreamDeckDeviceId {
    pub fn connect(&self) -> Result<StreamDeck, streamdeck::Error> {
        let mut sd = StreamDeck::connect(self.vid, self.pid, None)?;
        sd.set_blocking(false)?;
        Ok(sd)
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

    pub fn devices_in_use(&self) -> NonCryptoHashSet<StreamDeckDeviceId> {
        self.device_usage.values().copied().collect()
    }
}
