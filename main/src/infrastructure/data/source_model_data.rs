use super::none_if_minus_one;
use crate::application::{MidiSourceType, SourceCategory, SourceModel, VirtualControlElementType};
use crate::core::default_util::is_default;
use crate::core::notification;
use crate::domain::MappingCompartment;
use crate::infrastructure::data::VirtualControlElementIdData;
use helgoboss_learn::{MidiClockTransportMessage, OscTypeTag, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

/// This is the structure in which source settings are loaded and saved. It's optimized for being
/// represented as JSON. The JSON representation must be 100% backward-compatible.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub category: SourceCategory,
    // MIDI
    // midi_type would be a better name but we need backwards compatibility
    #[serde(default, skip_serializing_if = "is_default")]
    pub r#type: MidiSourceType,
    #[serde(
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_default"
    )]
    pub channel: Option<Channel>,
    #[serde(
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_default"
    )]
    pub number: Option<U14>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub character: SourceCharacter,
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_registered: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_14_bit: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub message: MidiClockTransportMessage,
    #[serde(default, skip_serializing_if = "is_default")]
    pub raw_midi_pattern: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub midi_script: String,
    // OSC
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_address_pattern: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_arg_index: Option<u32>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_arg_type: OscTypeTag,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_arg_is_relative: bool,
    // Virtual
    #[serde(default, skip_serializing_if = "is_default")]
    pub control_element_type: VirtualControlElementType,
    #[serde(default, skip_serializing_if = "is_default")]
    control_element_index: VirtualControlElementIdData,
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
            raw_midi_pattern: model.raw_midi_pattern.get_ref().clone(),
            midi_script: model.midi_script.get_ref().clone(),
            osc_address_pattern: model.osc_address_pattern.get_ref().clone(),
            osc_arg_index: model.osc_arg_index.get(),
            osc_arg_type: model.osc_arg_type_tag.get(),
            osc_arg_is_relative: model.osc_arg_is_relative.get(),
            control_element_type: model.control_element_type.get(),
            control_element_index: VirtualControlElementIdData::from_model(
                model.control_element_id.get(),
            ),
        }
    }

    pub fn apply_to_model(&self, model: &mut SourceModel, compartment: MappingCompartment) {
        self.apply_to_model_flexible(model, true, compartment, None);
    }

    /// Applies this data to the given source model. Doesn't proceed if data is invalid.
    pub fn apply_to_model_flexible(
        &self,
        model: &mut SourceModel,
        with_notification: bool,
        compartment: MappingCompartment,
        preset_version: Option<&Version>,
    ) {
        let final_category = if self.category.is_allowed_in(compartment) {
            self.category
        } else {
            SourceCategory::default_for(compartment)
        };
        model
            .category
            .set_with_optional_notification(final_category, with_notification);
        if self.r#type == MidiSourceType::ParameterNumberValue {
            model
                .parameter_number_message_number
                .set_with_optional_notification(self.number, with_notification)
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
            model
                .midi_message_number
                .set_with_optional_notification(number, with_notification);
        };
        model
            .midi_source_type
            .set_with_optional_notification(self.r#type, with_notification);
        model
            .channel
            .set_with_optional_notification(self.channel, with_notification);
        let character = if self.category == SourceCategory::Midi
            && self.r#type == MidiSourceType::ControlChangeValue
            && self.is_14_bit == Some(true)
        {
            // In old versions, 14-bit CC didn't support custom characters. We don't want it to
            // interfere even it was set.
            let is_old_preset = preset_version
                .map(|v| v < &Version::parse("2.8.0-rc.1").unwrap())
                .unwrap_or(true);
            if is_old_preset {
                SourceCharacter::RangeElement
            } else {
                self.character
            }
        } else {
            self.character
        };
        model
            .custom_character
            .set_with_optional_notification(character, with_notification);
        model
            .is_registered
            .set_with_optional_notification(self.is_registered, with_notification);
        model
            .is_14_bit
            .set_with_optional_notification(self.is_14_bit, with_notification);
        model
            .midi_clock_transport_message
            .set_with_optional_notification(self.message, with_notification);
        model
            .raw_midi_pattern
            .set_with_optional_notification(self.raw_midi_pattern.clone(), with_notification);
        model
            .midi_script
            .set_with_optional_notification(self.midi_script.clone(), with_notification);
        model
            .osc_address_pattern
            .set_with_optional_notification(self.osc_address_pattern.clone(), with_notification);
        model
            .osc_arg_index
            .set_with_optional_notification(self.osc_arg_index, with_notification);
        model
            .osc_arg_type_tag
            .set_with_optional_notification(self.osc_arg_type, with_notification);
        model
            .osc_arg_is_relative
            .set_with_optional_notification(self.osc_arg_is_relative, with_notification);
        model
            .control_element_type
            .set_with_optional_notification(self.control_element_type, with_notification);
        model.control_element_id.set_with_optional_notification(
            self.control_element_index.to_model(),
            with_notification,
        );
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
                character: SourceCharacter::RangeElement,
                is_registered: None,
                is_14_bit: Some(false),
                message: MidiClockTransportMessage::Start,
                raw_midi_pattern: "".to_owned(),
                midi_script: "".to_owned(),
                osc_address_pattern: "".to_owned(),
                osc_arg_index: None,
                osc_arg_type: Default::default(),
                osc_arg_is_relative: false,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: Default::default()
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
                character: SourceCharacter::RangeElement,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start,
                raw_midi_pattern: "".to_owned(),
                midi_script: "".to_owned(),
                osc_address_pattern: "".to_owned(),
                osc_arg_index: None,
                osc_arg_type: Default::default(),
                osc_arg_is_relative: false,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: Default::default()
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
            character: SourceCharacter::RangeElement,
            is_registered: Some(true),
            is_14_bit: Some(true),
            message: MidiClockTransportMessage::Start,
            raw_midi_pattern: "".to_owned(),
            osc_address_pattern: "".to_owned(),
            midi_script: "".to_owned(),
            osc_arg_index: None,
            osc_arg_type: Default::default(),
            osc_arg_is_relative: false,
            control_element_type: VirtualControlElementType::Multi,
            control_element_index: Default::default(),
        };
        let mut model = SourceModel::default();
        // When
        data.apply_to_model_flexible(&mut model, false, MappingCompartment::MainMappings, None);
        // Then
        assert_eq!(
            model.midi_source_type.get(),
            MidiSourceType::ParameterNumberValue
        );
        assert_eq!(model.channel.get(), Some(channel(8)));
        assert_eq!(model.midi_message_number.get(), None);
        assert_eq!(model.parameter_number_message_number.get(), None);
        assert_eq!(model.custom_character.get(), SourceCharacter::RangeElement);
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
            character: SourceCharacter::RangeElement,
            is_registered: None,
            is_14_bit: Some(false),
            message: MidiClockTransportMessage::Stop,
            raw_midi_pattern: "".to_owned(),
            midi_script: "".to_owned(),
            osc_address_pattern: "".to_owned(),
            osc_arg_index: None,
            osc_arg_type: Default::default(),
            osc_arg_is_relative: false,
            control_element_type: VirtualControlElementType::Multi,
            control_element_index: Default::default(),
        };
        let mut model = SourceModel::default();
        // When
        data.apply_to_model_flexible(&mut model, false, MappingCompartment::MainMappings, None);
        // Then
        assert_eq!(model.midi_source_type.get(), MidiSourceType::ClockTransport);
        assert_eq!(model.channel.get(), None);
        assert_eq!(model.midi_message_number.get(), Some(u7(112)));
        assert_eq!(model.parameter_number_message_number.get(), None);
        assert_eq!(model.custom_character.get(), SourceCharacter::RangeElement);
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
                raw_midi_pattern: "".to_owned(),
                midi_script: "".to_owned(),
                osc_address_pattern: "".to_owned(),
                osc_arg_index: Some(0),
                osc_arg_type: Default::default(),
                osc_arg_is_relative: false,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: Default::default(),
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
                raw_midi_pattern: "".to_owned(),
                midi_script: "".to_owned(),
                osc_address_pattern: "".to_owned(),
                osc_arg_index: Some(0),
                osc_arg_type: Default::default(),
                osc_arg_is_relative: false,
                control_element_type: VirtualControlElementType::Multi,
                control_element_index: Default::default(),
            }
        );
    }
}
