use crate::domain::{LifecycleMidiData, LifecycleMidiMessage, MappingExtension};

use crate::application::parse_hex_string;
use basedrop::Owned;
use helgoboss_learn::RawMidiEvent;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use wrap_debug::WrapDebug;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MappingExtensionModel {
    on_activate: LifecycleModel,
    on_deactivate: LifecycleModel,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct LifecycleModel {
    send_midi_feedback: Vec<LifecycleMidiMessageModel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LifecycleMidiMessageModel {
    Raw(RawMidiMessage),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String")]
struct RawHexStringMidiMessage(Vec<u8>);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RawByteArrayMidiMessage(Vec<u8>);

impl TryFrom<String> for RawHexStringMidiMessage {
    type Error = hex::FromHexError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let vec = parse_hex_string(&value)?;
        Ok(Self(vec))
    }
}

impl LifecycleMidiMessageModel {
    pub fn create_lifecycle_midi_message(
        &self,
        collector_handle: &basedrop::Handle,
    ) -> Result<LifecycleMidiMessage, &'static str> {
        use LifecycleMidiMessageModel::*;
        let message = match self {
            Raw(msg) => {
                let event = RawMidiEvent::try_from_slice(0, msg.bytes())?;
                LifecycleMidiMessage::Raw(WrapDebug(Owned::new(collector_handle, event)))
            }
        };
        Ok(message)
    }
}

impl MappingExtensionModel {
    pub fn create_mapping_extension(
        &self,
        collector_handle: &basedrop::Handle,
    ) -> Result<MappingExtension, &'static str> {
        fn convert_messages(
            model: &[LifecycleMidiMessageModel],
            collector_handle: &basedrop::Handle,
        ) -> Result<Vec<LifecycleMidiMessage>, &'static str> {
            model
                .iter()
                .map(|m| m.create_lifecycle_midi_message(collector_handle))
                .collect()
        }
        let ext = MappingExtension::new(LifecycleMidiData::new(
            collector_handle,
            convert_messages(&self.on_activate.send_midi_feedback, collector_handle)?,
            convert_messages(&self.on_deactivate.send_midi_feedback, collector_handle)?,
        ));
        Ok(ext)
    }
}
