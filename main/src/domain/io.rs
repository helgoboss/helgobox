use crate::domain::{MidiControlInput, MidiFeedbackOutput, OscDeviceId};

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ControlInput {
    Midi(MidiControlInput),
    Osc(OscDeviceId),
}

impl Default for ControlInput {
    fn default() -> Self {
        Self::Midi(MidiControlInput::FxInput)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeedbackOutput {
    Midi(MidiFeedbackOutput),
    Osc(OscDeviceId),
}
