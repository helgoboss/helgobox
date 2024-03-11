use crate::domain::{MidiControlInput, MidiDestination, OscDeviceId};
use reaper_medium::{MidiInputDeviceId, MidiOutputDeviceId};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ControlInput {
    Midi(MidiControlInput),
    Osc(OscDeviceId),
    Keyboard,
}

impl ControlInput {
    pub fn from_device_input(input: DeviceControlInput) -> Self {
        match input {
            DeviceControlInput::Midi(id) => Self::Midi(MidiControlInput::Device(id)),
            DeviceControlInput::Osc(id) => Self::Osc(id),
        }
    }

    pub fn midi_control_input(self) -> Option<MidiControlInput> {
        if let ControlInput::Midi(i) = self {
            Some(i)
        } else {
            None
        }
    }

    pub fn device_input(self) -> Option<DeviceControlInput> {
        use ControlInput::*;
        match self {
            Midi(MidiControlInput::Device(id)) => Some(DeviceControlInput::Midi(id)),
            Osc(id) => Some(DeviceControlInput::Osc(id)),
            _ => None,
        }
    }

    pub fn is_midi_device(self) -> bool {
        matches!(self, ControlInput::Midi(MidiControlInput::Device(_)))
    }

    pub fn is_midi_fx_input(self) -> bool {
        matches!(self, ControlInput::Midi(MidiControlInput::FxInput))
    }
}

impl Default for ControlInput {
    fn default() -> Self {
        Self::Midi(MidiControlInput::FxInput)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DeviceControlInput {
    Midi(MidiInputDeviceId),
    Osc(OscDeviceId),
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeedbackOutput {
    Midi(MidiDestination),
    Osc(OscDeviceId),
}

impl FeedbackOutput {
    pub fn from_device_output(output: DeviceFeedbackOutput) -> Self {
        match output {
            DeviceFeedbackOutput::Midi(id) => Self::Midi(MidiDestination::Device(id)),
            DeviceFeedbackOutput::Osc(id) => Self::Osc(id),
        }
    }

    pub fn midi_destination(&self) -> Option<MidiDestination> {
        if let Self::Midi(dest) = self {
            Some(*dest)
        } else {
            None
        }
    }

    pub fn device_output(self) -> Option<DeviceFeedbackOutput> {
        use FeedbackOutput::*;
        match self {
            Midi(MidiDestination::Device(id)) => Some(DeviceFeedbackOutput::Midi(id)),
            Osc(id) => Some(DeviceFeedbackOutput::Osc(id)),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DeviceFeedbackOutput {
    Midi(MidiOutputDeviceId),
    Osc(OscDeviceId),
}
