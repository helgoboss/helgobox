use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InstanceInfoEvent {
    Generic(GenericInstanceInfoEvent),
    /// If attempting to MIDI-learn but the track is either not armed or the input monitoring mode
    /// is not suitable.
    MidiLearnFromFxInputButTrackNotArmed,
    MidiLearnFromFxInputButTrackHasAudioInput,
}

impl InstanceInfoEvent {
    pub fn generic(message: impl Into<String>) -> Self {
        Self::Generic(GenericInstanceInfoEvent {
            message: message.into(),
        })
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct GenericInstanceInfoEvent {
    pub message: String,
}
