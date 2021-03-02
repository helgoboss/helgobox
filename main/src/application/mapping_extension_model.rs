use crate::domain::{LifecycleMidiData, LifecycleMidiMessage, MappingExtension, RawMidiData};
use helgoboss_midi::RawShortMessage;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MappingExtensionModel {
    on_activate: LifecycleModel,
    on_deactivate: LifecycleModel,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct LifecycleModel {
    send_midi_feedback: Vec<LifecycleMidiMessageModel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LifecycleMidiMessageModel {
    Raw(RawMidiMessage),
    Short(RawShortMessage),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum RawMidiMessage {
    HexString(RawHexStringMidiMessage),
    ByteArray(RawByteArrayMidiMessage),
}

impl RawMidiMessage {
    fn bytes(&self) -> &[u8] {
        use RawMidiMessage::*;
        match self {
            HexString(msg) => &msg.0,
            ByteArray(msg) => &msg.0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct RawHexStringMidiMessage(Vec<u8>);

#[derive(Debug, Serialize, Deserialize)]
struct RawByteArrayMidiMessage(Vec<u8>);

impl TryFrom<String> for RawHexStringMidiMessage {
    type Error = hex::FromHexError;

    fn try_from(mut value: String) -> Result<Self, Self::Error> {
        value.retain(|c| c != ' ');
        let vec = hex::decode(value)?;
        Ok(Self(vec))
    }
}

impl TryFrom<LifecycleMidiMessageModel> for LifecycleMidiMessage {
    type Error = &'static str;

    fn try_from(value: LifecycleMidiMessageModel) -> Result<Self, Self::Error> {
        use LifecycleMidiMessageModel::*;
        let message = match value {
            Raw(msg) => LifecycleMidiMessage::Raw(RawMidiData::try_from_slice(msg.bytes())?),
            Short(msg) => LifecycleMidiMessage::Short(msg),
        };
        Ok(message)
    }
}

impl TryFrom<MappingExtensionModel> for MappingExtension {
    type Error = &'static str;

    fn try_from(value: MappingExtensionModel) -> Result<Self, Self::Error> {
        fn convert_messages(
            model: Vec<LifecycleMidiMessageModel>,
        ) -> Result<Vec<LifecycleMidiMessage>, &'static str> {
            model
                .into_iter()
                .map(LifecycleMidiMessage::try_from)
                .collect()
        }
        let ext = MappingExtension::new(LifecycleMidiData {
            activation_midi_messages: convert_messages(value.on_activate.send_midi_feedback)?,
            deactivation_midi_messages: convert_messages(value.on_deactivate.send_midi_feedback)?,
        });
        Ok(ext)
    }
}
