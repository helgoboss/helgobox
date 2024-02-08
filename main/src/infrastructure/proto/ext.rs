use reaper_high::Reaper;
use reaper_medium::ReaperString;

use crate::infrastructure::data::{
    ControllerManager, FileBasedControllerPresetManager, FileBasedMainPresetManager,
};
use crate::infrastructure::plugin::InstanceShell;

use crate::infrastructure::proto::{
    event_reply, occasional_global_update, occasional_instance_update, AudioInputChannel,
    AudioInputChannels, ContinuousColumnUpdate, ContinuousMatrixUpdate,
    GetContinuousColumnUpdatesReply, GetContinuousMatrixUpdatesReply,
    GetContinuousSlotUpdatesReply, GetOccasionalClipUpdatesReply, GetOccasionalColumnUpdatesReply,
    GetOccasionalGlobalUpdatesReply, GetOccasionalInstanceUpdatesReply,
    GetOccasionalMatrixUpdatesReply, GetOccasionalRowUpdatesReply, GetOccasionalSlotUpdatesReply,
    GetOccasionalTrackUpdatesReply, MidiDeviceStatus, MidiInputDevice, MidiInputDevices,
    MidiOutputDevice, MidiOutputDevices, OccasionalGlobalUpdate, OccasionalInstanceUpdate,
    OccasionalMatrixUpdate, QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate,
    QualifiedOccasionalColumnUpdate, QualifiedOccasionalRowUpdate, QualifiedOccasionalSlotUpdate,
    QualifiedOccasionalTrackUpdate, SlotAddress,
};
use realearn_api::runtime::{ControllerPreset, MainPreset};

impl occasional_instance_update::Update {
    pub fn settings(instance_shell: &InstanceShell) -> Self {
        let settings = instance_shell.settings();
        let json =
            serde_json::to_string(&settings).expect("couldn't represent instance settings as JSON");
        Self::Settings(json)
    }
}

impl occasional_global_update::Update {
    pub fn info_event(event: realearn_api::runtime::InfoEvent) -> Self {
        let json =
            serde_json::to_string(&event).expect("couldn't represent main info event as JSON");
        Self::InfoEvent(json)
    }

    pub fn midi_input_devices() -> Self {
        Self::MidiInputDevices(MidiInputDevices::from_engine(
            Reaper::get()
                .midi_input_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn midi_output_devices() -> Self {
        Self::MidiOutputDevices(MidiOutputDevices::from_engine(
            Reaper::get()
                .midi_output_devices()
                .filter(|d| d.is_available()),
        ))
    }

    pub fn audio_input_channels() -> Self {
        Self::AudioInputChannels(AudioInputChannels::from_engine(
            Reaper::get().input_channels(),
        ))
    }

    pub fn controller_presets(manager: &FileBasedControllerPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| ControllerPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json = serde_json::to_string(&api_presets)
            .expect("couldn't represent controller presets as JSON");
        Self::ControllerPresets(json)
    }

    pub fn main_presets(manager: &FileBasedMainPresetManager) -> Self {
        let api_presets: Vec<_> = manager
            .preset_infos()
            .iter()
            .map(|info| MainPreset {
                id: info.common.id.clone(),
                common: info.common.meta_data.clone(),
                specific: info.specific_meta_data.clone(),
            })
            .collect();
        let json =
            serde_json::to_string(&api_presets).expect("couldn't represent main presets as JSON");
        Self::MainPresets(json)
    }

    pub fn controller_config(manager: &ControllerManager) -> Self {
        let json = serde_json::to_string(manager.controller_config())
            .expect("couldn't represent controller config as JSON");
        Self::ControllerConfig(json)
    }
}

impl MidiInputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiInputDevice>) -> Self {
        Self {
            devices: devs.map(MidiInputDevice::from_engine).collect(),
        }
    }
}

impl MidiInputDevice {
    pub fn from_engine(dev: reaper_high::MidiInputDevice) -> Self {
        MidiInputDevice {
            id: dev.id().get() as _,
            name: dev.name().into_string(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiOutputDevices {
    pub fn from_engine(devs: impl Iterator<Item = reaper_high::MidiOutputDevice>) -> Self {
        Self {
            devices: devs.map(MidiOutputDevice::from_engine).collect(),
        }
    }
}

impl MidiOutputDevice {
    pub fn from_engine(dev: reaper_high::MidiOutputDevice) -> Self {
        MidiOutputDevice {
            id: dev.id().get() as _,
            name: dev.name().into_string(),
            status: MidiDeviceStatus::from_engine(dev.is_open(), dev.is_connected()).into(),
        }
    }
}

impl MidiDeviceStatus {
    pub fn from_engine(open: bool, connected: bool) -> Self {
        use MidiDeviceStatus::*;
        match (open, connected) {
            (false, false) => Disconnected,
            (false, true) => ConnectedButDisabled,
            // Shouldn't happen but cope with it.
            (true, false) => Disconnected,
            (true, true) => Connected,
        }
    }
}

impl AudioInputChannels {
    pub fn from_engine(channels: impl Iterator<Item = ReaperString>) -> Self {
        Self {
            channels: channels
                .enumerate()
                .map(|(i, name)| AudioInputChannel {
                    index: i as u32,
                    name: name.into_string(),
                })
                .collect(),
        }
    }
}

impl SlotAddress {
    pub fn from_engine(address: playtime_api::persistence::SlotAddress) -> Self {
        Self {
            column_index: address.column() as _,
            row_index: address.row() as _,
        }
    }

    pub fn to_engine(&self) -> playtime_api::persistence::SlotAddress {
        playtime_api::persistence::SlotAddress::new(self.column_index as _, self.row_index as _)
    }
}

impl From<Vec<OccasionalGlobalUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalGlobalUpdate>) -> Self {
        event_reply::Value::OccasionalGlobalUpdatesReply(GetOccasionalGlobalUpdatesReply {
            global_updates: value,
        })
    }
}

impl From<Vec<OccasionalInstanceUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalInstanceUpdate>) -> Self {
        event_reply::Value::OccasionalInstanceUpdatesReply(GetOccasionalInstanceUpdatesReply {
            instance_updates: value,
        })
    }
}

impl From<Vec<OccasionalMatrixUpdate>> for event_reply::Value {
    fn from(value: Vec<OccasionalMatrixUpdate>) -> Self {
        event_reply::Value::OccasionalMatrixUpdatesReply(GetOccasionalMatrixUpdatesReply {
            matrix_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalTrackUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalTrackUpdate>) -> Self {
        event_reply::Value::OccasionalTrackUpdatesReply(GetOccasionalTrackUpdatesReply {
            track_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalColumnUpdate>) -> Self {
        event_reply::Value::OccasionalColumnUpdatesReply(GetOccasionalColumnUpdatesReply {
            column_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalRowUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalRowUpdate>) -> Self {
        event_reply::Value::OccasionalRowUpdatesReply(GetOccasionalRowUpdatesReply {
            row_updates: value,
        })
    }
}
impl From<Vec<QualifiedOccasionalSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalSlotUpdate>) -> Self {
        event_reply::Value::OccasionalSlotUpdatesReply(GetOccasionalSlotUpdatesReply {
            slot_updates: value,
        })
    }
}

impl From<Vec<QualifiedOccasionalClipUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedOccasionalClipUpdate>) -> Self {
        event_reply::Value::OccasionalClipUpdatesReply(GetOccasionalClipUpdatesReply {
            clip_updates: value,
        })
    }
}

impl From<ContinuousMatrixUpdate> for event_reply::Value {
    fn from(value: ContinuousMatrixUpdate) -> Self {
        event_reply::Value::ContinuousMatrixUpdatesReply(GetContinuousMatrixUpdatesReply {
            matrix_update: Some(value),
        })
    }
}

impl From<Vec<ContinuousColumnUpdate>> for event_reply::Value {
    fn from(value: Vec<ContinuousColumnUpdate>) -> Self {
        event_reply::Value::ContinuousColumnUpdatesReply(GetContinuousColumnUpdatesReply {
            column_updates: value,
        })
    }
}

impl From<Vec<QualifiedContinuousSlotUpdate>> for event_reply::Value {
    fn from(value: Vec<QualifiedContinuousSlotUpdate>) -> Self {
        event_reply::Value::ContinuousSlotUpdatesReply(GetContinuousSlotUpdatesReply {
            slot_updates: value,
        })
    }
}
