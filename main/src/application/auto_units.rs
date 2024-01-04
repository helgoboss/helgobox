use crate::domain::{DeviceControlInput, DeviceFeedbackOutput, OscDeviceId};
use realearn_api::persistence::{Controller, ControllerConnection, PresetId};
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::str::FromStr;

pub fn determine_auto_units(controllers: &[Controller]) -> Vec<AutoUnitData> {
    controllers
        .iter()
        .filter_map(|controller| {
            let auto_data = AutoControllerData::from_controller(&controller)?;
            Some((controller, auto_data))
        })
        .flat_map(|(controller, auto_data)| {
            // Translate roles into auto units
            [
                (ControllerRoleKind::Daw, &controller.roles.daw),
                (ControllerRoleKind::Clip, &controller.roles.clip),
            ]
            .into_iter()
            .filter_map(move |(role_kind, role)| {
                let role = role.as_ref()?;
                let preset_id = role.main_preset.as_ref().map(|id| id.get().to_string())?;
                let u = AutoUnitData {
                    controller: auto_data.clone(),
                    role_kind,
                    preset_id,
                };
                Some(u)
            })
        })
        .collect()
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AutoUnitData {
    pub controller: AutoControllerData,
    pub role_kind: ControllerRoleKind,
    pub preset_id: String,
}

impl AutoUnitData {
    pub fn matches_installed(&self, installed: &Self) -> bool {
        self.controller.controller_id == installed.controller.controller_id
            && self.role_kind == installed.role_kind
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AutoControllerData {
    pub controller_id: String,
    pub controller_preset: Option<PresetId>,
    pub input: Option<DeviceControlInput>,
    pub output: Option<DeviceFeedbackOutput>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ControllerRoleKind {
    Clip,
    Daw,
}

impl AutoControllerData {
    pub fn from_controller(controller: &Controller) -> Option<AutoControllerData> {
        // Ignore if no connection info
        let connection = controller.connection.as_ref()?;
        // Translate connection info
        let (input, output) = match connection {
            ControllerConnection::Midi(c) => {
                let input = c
                    .input_port
                    .map(|p| DeviceControlInput::Midi(MidiInputDeviceId::new(p.get() as u8)));
                let output = c
                    .output_port
                    .map(|p| DeviceFeedbackOutput::Midi(MidiOutputDeviceId::new(p.get() as u8)));
                (input, output)
            }
            ControllerConnection::Osc(c) => {
                let dev_id = c
                    .osc_device_id
                    .as_ref()
                    .and_then(|id| OscDeviceId::from_str(id.get()).ok());
                if let Some(dev_id) = dev_id {
                    (
                        Some(DeviceControlInput::Osc(dev_id)),
                        Some(DeviceFeedbackOutput::Osc(dev_id)),
                    )
                } else {
                    (None, None)
                }
            }
        };
        // Ignore if neither input nor output given
        if input.is_none() && output.is_none() {
            return None;
        }
        // Ignore if input not connected
        if let Some(input) = input {
            if !input_is_connected(input) {
                return None;
            }
        }
        // Ignore if output not connected
        if let Some(output) = output {
            if !output_is_connected(output) {
                return None;
            }
        }
        // Build data
        let data = Self {
            controller_id: controller.id.clone(),
            controller_preset: controller.controller_preset.clone(),
            input,
            output,
        };
        Some(data)
    }
}

fn input_is_connected(input: DeviceControlInput) -> bool {
    match input {
        DeviceControlInput::Midi(id) => MidiInputDevice::new(id).is_connected(),
        DeviceControlInput::Osc(id) => osc_device_is_connected(id),
    }
}

fn output_is_connected(output: DeviceFeedbackOutput) -> bool {
    match output {
        DeviceFeedbackOutput::Midi(id) => MidiOutputDevice::new(id).is_connected(),
        DeviceFeedbackOutput::Osc(id) => osc_device_is_connected(id),
    }
}

fn osc_device_is_connected(_dev_id: OscDeviceId) -> bool {
    // No easy way to check with OSC. Just return true for now.
    true
}
