use crate::domain::{MidiSourceModel, MidiSourceType};
use helgoboss_learn::{MidiClockTransportMessage, SourceCharacter};
use helgoboss_midi::U7;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use validator::{Validate, ValidationError, ValidationErrors};
use validator_derive::*;

/// This is the structure in which source settings are loaded and saved. It's optimized for being
/// represented as JSON. The JSON representation must be 100% backward-compatible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase", default)]
#[validate(schema(function = "validate_schema"))]
pub struct SourceModelData {
    pub r#type: MidiSourceType,
    #[validate(range(min = -1, max = 15))]
    pub channel: Option<i16>,
    #[validate(range(min = -1, max = 16383))]
    pub number: Option<i32>,
    pub character: SourceCharacter,
    pub is_registered: Option<bool>,
    pub is_14_bit: Option<bool>,
    pub message: MidiClockTransportMessage,
}

impl Default for SourceModelData {
    fn default() -> Self {
        Self {
            r#type: MidiSourceType::ControlChangeValue,
            channel: Some(0),
            number: Some(0),
            character: SourceCharacter::Range,
            is_registered: Some(false),
            is_14_bit: Some(false),
            message: MidiClockTransportMessage::Start,
        }
    }
}

fn validate_schema(data: &SourceModelData) -> Result<(), ValidationError> {
    if data.r#type != MidiSourceType::ParameterNumberValue
        && data.number.map(|n| n <= U7::MAX.get() as i32) == Some(false)
    {
        let mut error = ValidationError::new("number_too_large");
        error.add_param("number".into(), &data.number);
        return Err(error);
    }
    Ok(())
}

impl SourceModelData {
    pub fn from_model(model: &MidiSourceModel) -> Self {
        Self {
            r#type: model.r#type.get(),
            channel: model.channel.get().map(|ch| ch.into()),
            number: if model.r#type.get() == MidiSourceType::ParameterNumberValue {
                model
                    .parameter_number_message_number
                    .get()
                    .map(|n| n.into())
            } else {
                model.midi_message_number.get().map(|n| n.into())
            },
            character: model.custom_character.get(),
            is_registered: model.is_registered.get(),
            is_14_bit: model.is_14_bit.get(),
            message: model.midi_clock_transport_message.get(),
        }
    }

    /// Applies this data to the given source model. Doesn't proceed if data is invalid.
    pub fn apply_to_model(&self, model: &mut MidiSourceModel) -> Result<(), &'static str> {
        // Validation
        let channel = match self.channel.none_if_negative() {
            None => None,
            Some(v) => Some(v.try_into().map_err(|_| "invalid channel")?),
        };
        // Mutation
        model.r#type.set(self.r#type);
        model.channel.set(channel);
        if self.r#type == MidiSourceType::ParameterNumberValue {
            model.parameter_number_message_number.set(
                self.number
                    .none_if_negative()
                    .map(|v| v.try_into().unwrap()),
            )
        } else {
            model.midi_message_number.set(
                self.number
                    .none_if_negative()
                    .map(|v| v.try_into().unwrap()),
            )
        }
        model.custom_character.set(self.character);
        model.is_registered.set(self.is_registered);
        model.is_14_bit.set(self.is_14_bit);
        model.midi_clock_transport_message.set(self.message);
        Ok(())
    }
}

trait NoneIfNegative {
    fn none_if_negative(self) -> Self;
}

impl<T: PartialOrd + From<i8> + Copy> NoneIfNegative for Option<T> {
    fn none_if_negative(self) -> Self {
        match self {
            Some(v) if v >= 0.into() => self,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_midi::test_util::*;
    use serde_json::json;
    use validator::ValidationErrors;

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
                r#type: MidiSourceType::ControlChangeValue,
                channel: Some(0),
                number: Some(0),
                character: SourceCharacter::Range,
                is_registered: Some(false),
                is_14_bit: Some(false),
                message: MidiClockTransportMessage::Start
            }
        );
        assert!(data.validate().is_ok());
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
                r#type: MidiSourceType::ParameterNumberValue,
                channel: Some(-1),
                number: Some(12542),
                character: SourceCharacter::Range,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start
            }
        );
        assert!(data.validate().is_ok());
    }

    #[test]
    fn validate_1() {
        // Given
        let data = SourceModelData {
            r#type: MidiSourceType::ParameterNumberValue,
            channel: Some(-4),
            number: Some(21000),
            character: SourceCharacter::Switch,
            is_registered: Some(true),
            is_14_bit: Some(true),
            message: MidiClockTransportMessage::Continue,
        };
        // When
        let result: Result<(), ValidationErrors> = data.validate();
        // Then
        assert!(result.is_err());
        let err = result.unwrap_err();
        let errors = err.errors();
        assert_eq!(errors.len(), 2);
        assert!(errors.contains_key("channel"));
        assert!(errors.contains_key("number"));
    }

    #[test]
    fn validate_2() {
        // Given
        let data = SourceModelData {
            r#type: MidiSourceType::ControlChangeValue,
            channel: Some(-1),
            number: Some(500),
            character: SourceCharacter::Switch,
            is_registered: Some(false),
            is_14_bit: None,
            message: MidiClockTransportMessage::Start,
        };
        // When
        let result: Result<(), ValidationErrors> = data.validate();
        // Then
        assert!(result.is_err());
        let err = result.unwrap_err();
        let errors = err.errors();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn apply_1() {
        // Given
        let data = SourceModelData {
            r#type: MidiSourceType::ParameterNumberValue,
            channel: Some(8),
            number: Some(-1),
            character: SourceCharacter::Range,
            is_registered: Some(true),
            is_14_bit: Some(true),
            message: MidiClockTransportMessage::Start,
        };
        let mut model = MidiSourceModel::default();
        // When
        let result = data.apply_to_model(&mut model);
        // Then
        assert!(result.is_ok());
        assert_eq!(model.r#type.get(), MidiSourceType::ParameterNumberValue);
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
            r#type: MidiSourceType::ClockTransport,
            channel: None,
            number: Some(112),
            character: SourceCharacter::Range,
            is_registered: None,
            is_14_bit: Some(false),
            message: MidiClockTransportMessage::Stop,
        };
        let mut model = MidiSourceModel::default();
        // When
        let result = data.apply_to_model(&mut model);
        assert!(result.is_ok());
        // Then
        assert_eq!(model.r#type.get(), MidiSourceType::ClockTransport);
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
        let mut model = MidiSourceModel::default();
        model.r#type.set(MidiSourceType::ControlChangeValue);
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
                r#type: MidiSourceType::ControlChangeValue,
                channel: Some(15),
                number: Some(12),
                character: SourceCharacter::Encoder2,
                is_registered: Some(false),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start,
            }
        );
    }

    #[test]
    fn from_2() {
        // Given
        let mut model = MidiSourceModel::default();
        model.r#type.set(MidiSourceType::ParameterNumberValue);
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
                r#type: MidiSourceType::ParameterNumberValue,
                channel: None,
                number: Some(78),
                character: SourceCharacter::Encoder1,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Continue,
            }
        );
    }
}
