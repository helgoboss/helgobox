use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InstanceInfoEvent {
    /// If attempting to MIDI-learn but the track is either not armed or the input monitoring mode
    /// is not suitable.
    MidiLearnFromFxInputButTrackNotArmed,
    MidiLearnFromFxInputButTrackHasAudioInput,
}
