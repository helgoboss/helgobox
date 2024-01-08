use crate::domain::{
    ControlInput, DeviceControlInput, DeviceFeedbackOutput, FeedbackOutput, OscDeviceId,
};
use realearn_api::persistence::{Controller, ControllerConnection};
use reaper_high::{MidiInputDevice, MidiOutputDevice};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};
use std::str::FromStr;

pub fn determine_auto_units(controllers: &[Controller]) -> Vec<AutoUnitData> {
    controllers
        .iter()
        .filter_map(AutoUnitData::from_controller)
        .collect()
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AutoUnitData {
    pub controller_id: String,
    pub input: Option<DeviceControlInput>,
    pub output: Option<DeviceFeedbackOutput>,
    pub controller_preset_id: Option<String>,
    pub main_preset_id: String,
}

impl AutoUnitData {
    pub fn from_controller(controller: &Controller) -> Option<Self> {
        // Ignore if no connection info or no main preset
        let connection = controller.connection.as_ref()?;
        let main_preset = controller.default_main_preset.as_ref()?;
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
            controller_preset_id: controller
                .default_controller_preset
                .as_ref()
                .map(|id| id.get().to_string()),
            input,
            output,
            main_preset_id: main_preset.get().to_string(),
        };
        Some(data)
    }

    pub fn control_input(&self) -> ControlInput {
        self.input
            .map(ControlInput::from_device_input)
            .unwrap_or_default()
    }

    pub fn feedback_output(&self) -> Option<FeedbackOutput> {
        self.output.map(FeedbackOutput::from_device_output)
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
