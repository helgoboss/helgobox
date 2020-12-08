use super::none_if_minus_one;
use crate::application::{MidiSourceType, SourceCategory, SourceModel, VirtualControlElementType};
use crate::core::default_util::{is_default, is_none_or_some_default, some_default};
use crate::core::notification;
use helgoboss_learn::{MidiClockTransportMessage, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

/// This is the structure in which source settings are loaded and saved. It's optimized for being
/// represented as JSON. The JSON representation must be 100% backward-compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub category: SourceCategory,
    // midi_type would be a better name but we need backwards compatibility
    #[serde(default, skip_serializing_if = "is_default")]
    pub r#type: MidiSourceType,
    #[serde(
        deserialize_with = "none_if_minus_one",
        default = "some_default",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub channel: Option<Channel>,
    #[serde(
        deserialize_with = "none_if_minus_one",
        default = "some_default",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub number: Option<U14>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub character: SourceCharacter,
    #[serde(
        default = "some_default",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub is_registered: Option<bool>,
    #[serde(
        default = "some_default",
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub is_14_bit: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub message: MidiClockTransportMessage,
    #[serde(default, skip_serializing_if = "is_default")]
    pub control_element_type: VirtualControlElementType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub control_element_index: u32,
}

impl SourceModelData {
    pub fn from_model(model: &SourceModel) -> Self {
        Self {
            category: model.category.get(),
            r#type: model.midi_source_type.get(),
            channel: model.channel.get(),
            number: if model.midi_source_type.get() == MidiSourceType::ParameterNumberValue {
                model.parameter_number_message_number.get()
            } else {
                model.midi_message_number.get().map(|n| n.into())
            },
            character: model.custom_character.get(),
            is_registered: model.is_registered.get(),
            is_14_bit: model.is_14_bit.get(),
            message: model.midi_clock_transport_message.get(),
            control_element_type: model.control_element_type.get(),
            control_element_index: model.control_element_index.get(),
        }
    }

    /// Applies this data to the given source model. Doesn't proceed if data is invalid.
    pub fn apply_to_model(&self, model: &mut SourceModel) {
        model.category.set_without_notification(self.category);
        if self.r#type == MidiSourceType::ParameterNumberValue {
            model
                .parameter_number_message_number
                .set_without_notification(self.number)
        } else {
            let number: Option<U7> = match self.number {
                None => None,
                Some(v) => match v.try_into() {
                    Ok(number) => Some(number),
                    Err(_) => {
                        notification::warn("MIDI message number too high");
                        None
                    }
                },
            };
            model.midi_message_number.set_without_notification(number);
        };
        model.midi_source_type.set_without_notification(self.r#type);
        model.channel.set_without_notification(self.channel);
        model
            .custom_character
            .set_without_notification(self.character);
        model
            .is_registered
            .set_without_notification(self.is_registered);
        model.is_14_bit.set_without_notification(self.is_14_bit);
        model
            .midi_clock_transport_message
            .set_without_notification(self.message);
        model
            .control_element_type
            .set_without_notification(self.control_element_type);
        model
            .control_element_index
            .set_without_notification(self.control_element_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::test_util::*;
    use serde_json::json;

    #[test]
    fn deserialize_1() {
        // Given
        let json = json!(
            {
                "channel": 0,
                "character": 0,
                "is14Bit": false,
                "number": 0,
                "type": 0
            }
        );
        // When
        let data: SourceModelData = serde_json::from_value(json).unwrap();
        // Then
        assert_eq!(
            data,
            SourceModelData {
                category: SourceCategory::Midi,
                r#type: MidiSourceType::ControlChangeValue,
                channel: Some(Channel::new(0)),
                number: Some(U14::new(0)),
                character: SourceCharacter::Range,
                is_registered: Some(false),
                is_14_bit: Some(false),
                message: MidiClockTransportMessage::Start,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: 0
            }
        );
    }

    #[test]
    fn deserialize_2() {
        // Given
        let json = json!(
            {
                "channel": -1,
                "is14Bit": true,
                "isRegistered": true,
                "number": 12542,
                "type": 6
            }
        );
        // When
        let data: SourceModelData = serde_json::from_value(json).unwrap();
        // Then
        assert_eq!(
            data,
            SourceModelData {
                category: SourceCategory::Midi,
                r#type: MidiSourceType::ParameterNumberValue,
                channel: None,
                number: Some(U14::new(12542)),
                character: SourceCharacter::Range,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: 0
            }
        );
    }

    #[test]
    fn apply_1() {
        // Given
        let data = SourceModelData {
            category: SourceCategory::Midi,
            r#type: MidiSourceType::ParameterNumberValue,
            channel: Some(Channel::new(8)),
            number: None,
            character: SourceCharacter::Range,
            is_registered: Some(true),
            is_14_bit: Some(true),
            message: MidiClockTransportMessage::Start,
            control_element_type: VirtualControlElementType::Multi,
            control_element_index: 0,
        };
        let mut model = SourceModel::default();
        // When
        data.apply_to_model(&mut model);
        // Then
        assert_eq!(
            model.midi_source_type.get(),
            MidiSourceType::ParameterNumberValue
        );
        assert_eq!(model.channel.get(), Some(channel(8)));
        assert_eq!(model.midi_message_number.get(), None);
        assert_eq!(model.parameter_number_message_number.get(), None);
        assert_eq!(model.custom_character.get(), SourceCharacter::Range);
        assert_eq!(
            model.midi_clock_transport_message.get(),
            MidiClockTransportMessage::Start
        );
        assert_eq!(model.is_registered.get(), Some(true));
        assert_eq!(model.is_14_bit.get(), Some(true));
    }

    #[test]
    fn apply_2() {
        // Given
        let data = SourceModelData {
            category: SourceCategory::Midi,
            r#type: MidiSourceType::ClockTransport,
            channel: None,
            number: Some(U14::new(112)),
            character: SourceCharacter::Range,
            is_registered: None,
            is_14_bit: Some(false),
            message: MidiClockTransportMessage::Stop,
            control_element_type: VirtualControlElementType::Multi,
            control_element_index: 0,
        };
        let mut model = SourceModel::default();
        // When
        data.apply_to_model(&mut model);
        // Then
        assert_eq!(model.midi_source_type.get(), MidiSourceType::ClockTransport);
        assert_eq!(model.channel.get(), None);
        assert_eq!(model.midi_message_number.get(), Some(u7(112)));
        assert_eq!(model.parameter_number_message_number.get(), None);
        assert_eq!(model.custom_character.get(), SourceCharacter::Range);
        assert_eq!(
            model.midi_clock_transport_message.get(),
            MidiClockTransportMessage::Stop
        );
        assert_eq!(model.is_registered.get(), None);
        assert_eq!(model.is_14_bit.get(), Some(false));
    }

    #[test]
    fn from_1() {
        // Given
        let mut model = SourceModel::default();
        model
            .midi_source_type
            .set(MidiSourceType::ControlChangeValue);
        model.channel.set(Some(channel(15)));
        model.midi_message_number.set(Some(u7(12)));
        model.custom_character.set(SourceCharacter::Encoder2);
        model.is_14_bit.set(Some(true));
        // When
        let data = SourceModelData::from_model(&model);
        // Then
        assert_eq!(
            data,
            SourceModelData {
                category: SourceCategory::Midi,
                r#type: MidiSourceType::ControlChangeValue,
                channel: Some(Channel::new(15)),
                number: Some(U14::new(12)),
                character: SourceCharacter::Encoder2,
                is_registered: Some(false),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: 0
            }
        );
    }

    #[test]
    fn from_2() {
        // Given
        let mut model = SourceModel::default();
        model
            .midi_source_type
            .set(MidiSourceType::ParameterNumberValue);
        model.channel.set(None);
        model.midi_message_number.set(Some(u7(77)));
        model.parameter_number_message_number.set(Some(u14(78)));
        model.custom_character.set(SourceCharacter::Encoder1);
        model.is_14_bit.set(Some(true));
        model.is_registered.set(Some(true));
        model
            .midi_clock_transport_message
            .set(MidiClockTransportMessage::Continue);
        // When
        let data = SourceModelData::from_model(&model);
        // Then
        assert_eq!(
            data,
            SourceModelData {
                category: SourceCategory::Midi,
                r#type: MidiSourceType::ParameterNumberValue,
                channel: None,
                number: Some(U14::new(78)),
                character: SourceCharacter::Encoder1,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Continue,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: 0
            }
        );
    }
}
