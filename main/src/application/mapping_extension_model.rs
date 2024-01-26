use crate::domain::{
    parse_hex_string, DisplayAsPrettyHex, LifecycleMidiData, LifecycleMidiMessage, MappingExtension,
};

use helgoboss_learn::RawMidiEvent;
use serde::{Deserialize, Serialize};
use serde_with::SerializeDisplay;
use std::convert::TryFrom;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MappingExtensionModel {
    pub on_activate: LifecycleModel,
    pub on_deactivate: LifecycleModel,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LifecycleModel {
    pub send_midi_feedback: Vec<LifecycleMidiMessageModel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleMidiMessageModel {
    Raw(RawMidiMessage),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RawMidiMessage {
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

#[derive(Clone, Debug, SerializeDisplay, Deserialize)]
#[serde(try_from = "String")]
pub struct RawHexStringMidiMessage(pub Vec<u8>);

impl Display for RawHexStringMidiMessage {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        DisplayAsPrettyHex(&self.0).fmt(f)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawByteArrayMidiMessage(pub Vec<u8>);

impl TryFrom<String> for RawHexStringMidiMessage {
    type Error = hex::FromHexError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let vec = parse_hex_string(&value)?;
        Ok(Self(vec))
    }
}

impl LifecycleMidiMessageModel {
    pub fn create_lifecycle_midi_message(&self) -> Result<LifecycleMidiMessage, &'static str> {
        use LifecycleMidiMessageModel::*;
        let message = match self {
            Raw(msg) => {
                let event = RawMidiEvent::try_from_slice(0, msg.bytes())?;
                LifecycleMidiMessage::Raw(Box::new(event))
            }
        };
        Ok(message)
    }
}

impl MappingExtensionModel {
    pub fn create_mapping_extension(&self) -> Result<MappingExtension, &'static str> {
        fn convert_messages(
            model: &[LifecycleMidiMessageModel],
        ) -> Result<Vec<LifecycleMidiMessage>, &'static str> {
            model
                .iter()
                .map(|m| m.create_lifecycle_midi_message())
                .collect()
        }
        let ext = MappingExtension::new(LifecycleMidiData {
            activation_midi_messages: convert_messages(&self.on_activate.send_midi_feedback)?,
            deactivation_midi_messages: convert_messages(&self.on_deactivate.send_midi_feedback)?,
        });
        Ok(ext)
    }
}
