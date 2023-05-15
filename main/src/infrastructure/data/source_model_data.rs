use super::none_if_minus_one;
use crate::application::{
    Change, MidiSourceType, ReaperSourceType, SourceCategory, SourceCommand, SourceModel,
    VirtualControlElementType,
};
use crate::base::notification;
use crate::domain::{Compartment, CompartmentParamIndex, Keystroke};
use crate::infrastructure::data::common::OscValueRange;
use crate::infrastructure::data::VirtualControlElementIdData;
use base::default_util::{deserialize_null_default, is_default};
use helgoboss_learn::{DisplayType, MidiClockTransportMessage, OscTypeTag, SourceCharacter};
use helgoboss_midi::{Channel, U14, U7};
use realearn_api::persistence::MidiScriptKind;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

/// This is the structure in which source settings are loaded and saved. It's optimized for being
/// represented as JSON. The JSON representation must be 100% backward-compatible.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceModelData {
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub category: SourceCategory,
    // MIDI
    // midi_type would be a better name but we need backwards compatibility
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
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
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub character: SourceCharacter,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub is_registered: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub is_14_bit: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub message: MidiClockTransportMessage,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub raw_midi_pattern: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub midi_script_kind: MidiScriptKind,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub midi_script: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub display_type: DisplayType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub display_id: Option<u8>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub line: Option<u8>,
    // OSC
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_address_pattern: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_index: Option<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_type: OscTypeTag,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_is_relative: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_arg_value_range: OscValueRange,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub osc_feedback_args: Vec<String>,
    // Keyboard
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub keystroke: Option<Keystroke>,
    // Virtual
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub control_element_type: VirtualControlElementType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub control_element_index: VirtualControlElementIdData,
    // REAPER
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub reaper_source_type: ReaperSourceType,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub timer_millis: u64,
    #[serde(
        default,
        deserialize_with = "deserialize_null_default",
        skip_serializing_if = "is_default"
    )]
    pub parameter_index: CompartmentParamIndex,
}

impl SourceModelData {
    pub fn from_model(model: &SourceModel) -> Self {
        Self {
            category: model.category(),
            r#type: model.midi_source_type(),
            channel: model.channel(),
            number: if model.midi_source_type() == MidiSourceType::ParameterNumberValue {
                model.parameter_number_message_number()
            } else {
                model.midi_message_number().map(|n| n.into())
            },
            character: model.custom_character(),
            is_registered: model.is_registered(),
            is_14_bit: model.is_14_bit(),
            message: model.midi_clock_transport_message(),
            raw_midi_pattern: model.raw_midi_pattern().to_owned(),
            midi_script_kind: model.midi_script_kind(),
            midi_script: model.midi_script().to_owned(),
            display_type: model.display_type(),
            display_id: model.display_id(),
            line: model.line(),
            osc_address_pattern: model.osc_address_pattern().to_owned(),
            osc_arg_index: model.osc_arg_index(),
            osc_arg_type: model.osc_arg_type_tag(),
            osc_arg_is_relative: model.osc_arg_is_relative(),
            osc_arg_value_range: OscValueRange::from_interval(model.osc_arg_value_range()),
            osc_feedback_args: model.osc_feedback_args().to_vec(),
            keystroke: model.keystroke(),
            control_element_type: model.control_element_type(),
            control_element_index: VirtualControlElementIdData::from_model(
                model.control_element_id(),
            ),
            reaper_source_type: model.reaper_source_type(),
            timer_millis: model.timer_millis(),
            parameter_index: model.parameter_index(),
        }
    }

    pub fn apply_to_model(&self, model: &mut SourceModel, compartment: Compartment) {
        self.apply_to_model_flexible(model, compartment, None);
    }

    /// Applies this data to the given source model. Doesn't proceed if data is invalid.
    pub fn apply_to_model_flexible(
        &self,
        model: &mut SourceModel,
        compartment: Compartment,
        preset_version: Option<&Version>,
    ) {
        use SourceCommand as P;
        let final_category = if self.category.is_allowed_in(compartment) {
            self.category
        } else {
            SourceCategory::default_for(compartment)
        };
        model.change(P::SetCategory(final_category));
        if self.r#type == MidiSourceType::ParameterNumberValue {
            model.change(P::SetParameterNumberMessageNumber(self.number));
        } else {
            let number: Option<U7> = match self.number {
                None => None,
                Some(v) => match v.try_into() {
                    Ok(number) => Some(number),
                    Err(_) => {
                        notification::warn("MIDI message number too high".to_string());
                        None
                    }
                },
            };
            model.change(P::SetMidiMessageNumber(number));
        };
        model.change(P::SetMidiSourceType(self.r#type));
        model.change(P::SetChannel(self.channel));
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
        model.change(P::SetCustomCharacter(character));
        model.change(P::SetIsRegistered(self.is_registered));
        model.change(P::SetIs14Bit(self.is_14_bit));
        model.change(P::SetMidiClockTransportMessage(self.message));
        model.change(P::SetRawMidiPattern(self.raw_midi_pattern.clone()));
        model.change(P::SetMidiScriptKind(self.midi_script_kind));
        model.change(P::SetMidiScript(self.midi_script.clone()));
        model.change(P::SetDisplayType(self.display_type));
        model.change(P::SetDisplayId(self.display_id));
        model.change(P::SetLine(self.line));
        model.change(P::SetOscAddressPattern(self.osc_address_pattern.clone()));
        model.change(P::SetOscArgIndex(self.osc_arg_index));
        model.change(P::SetOscArgTypeTag(self.osc_arg_type));
        model.change(P::SetOscArgIsRelative(self.osc_arg_is_relative));
        model.change(P::SetOscArgValueRange(
            self.osc_arg_value_range.to_interval(),
        ));
        model.change(P::SetOscFeedbackArgs(self.osc_feedback_args.clone()));
        model.change(P::SetControlElementType(self.control_element_type));
        model.change(P::SetControlElementId(
            self.control_element_index.to_model(),
        ));
        model.change(P::SetReaperSourceType(self.reaper_source_type));
        model.change(P::SetTimerMillis(self.timer_millis));
        model.change(P::SetParameterIndex(self.parameter_index));
        model.change(P::SetKeystroke(self.keystroke));
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
                control_element_type: VirtualControlElementType::Multi,
                ..Default::default()
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
                number: Some(U14::new(12542)),
                character: SourceCharacter::RangeElement,
                is_registered: Some(true),
                is_14_bit: Some(true),
                message: MidiClockTransportMessage::Start,
                control_element_type: VirtualControlElementType::Multi,
                ..Default::default()
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
            character: SourceCharacter::RangeElement,
            is_registered: Some(true),
            is_14_bit: Some(true),
            message: MidiClockTransportMessage::Start,
            control_element_type: VirtualControlElementType::Multi,
            ..Default::default()
        };
        let mut model = SourceModel::new();
        // When
        data.apply_to_model_flexible(&mut model, Compartment::Main, None);
        // Then
        assert_eq!(
            model.midi_source_type(),
            MidiSourceType::ParameterNumberValue
        );
        assert_eq!(model.channel(), Some(channel(8)));
        assert_eq!(model.midi_message_number(), None);
        assert_eq!(model.parameter_number_message_number(), None);
        assert_eq!(model.custom_character(), SourceCharacter::RangeElement);
        assert_eq!(
            model.midi_clock_transport_message(),
            MidiClockTransportMessage::Start
        );
        assert_eq!(model.is_registered(), Some(true));
        assert_eq!(model.is_14_bit(), Some(true));
    }

    #[test]
    fn apply_2() {
        // Given
        let data = SourceModelData {
            category: SourceCategory::Midi,
            r#type: MidiSourceType::ClockTransport,
            number: Some(U14::new(112)),
            character: SourceCharacter::RangeElement,
            is_14_bit: Some(false),
            message: MidiClockTransportMessage::Stop,
            control_element_type: VirtualControlElementType::Multi,
            ..Default::default()
        };
        let mut model = SourceModel::new();
        // When
        data.apply_to_model_flexible(&mut model, Compartment::Main, None);
        // Then
        assert_eq!(model.midi_source_type(), MidiSourceType::ClockTransport);
        assert_eq!(model.channel(), None);
        assert_eq!(model.midi_message_number(), Some(u7(112)));
        assert_eq!(model.parameter_number_message_number(), None);
        assert_eq!(model.custom_character(), SourceCharacter::RangeElement);
        assert_eq!(
            model.midi_clock_transport_message(),
            MidiClockTransportMessage::Stop
        );
        assert_eq!(model.is_registered(), None);
        assert_eq!(model.is_14_bit(), Some(false));
    }

    #[test]
    fn from_1() {
        // Given
        use SourceCommand as C;
        let mut model = SourceModel::new();
        model.change(C::SetCategory(SourceCategory::Midi));
        model.change(C::SetMidiSourceType(MidiSourceType::ControlChangeValue));
        model.change(C::SetChannel(Some(channel(15))));
        model.change(C::SetMidiMessageNumber(Some(u7(12))));
        model.change(C::SetCustomCharacter(SourceCharacter::Encoder2));
        model.change(C::SetIs14Bit(Some(true)));
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
                osc_arg_index: Some(0),
                control_element_type: VirtualControlElementType::Multi,
                ..Default::default()
            }
        );
    }

    #[test]
    fn from_2() {
        // Given
        use SourceCommand as C;
        let mut model = SourceModel::new();
        model.change(C::SetCategory(SourceCategory::Midi));
        model.change(C::SetMidiSourceType(MidiSourceType::ParameterNumberValue));
        model.change(C::SetChannel(None));
        model.change(C::SetMidiMessageNumber(Some(u7(77))));
        model.change(C::SetParameterNumberMessageNumber(Some(u14(78))));
        model.change(C::SetCustomCharacter(SourceCharacter::Encoder1));
        model.change(C::SetIs14Bit(Some(true)));
        model.change(C::SetIsRegistered(Some(true)));
        model.change(C::SetMidiClockTransportMessage(
            MidiClockTransportMessage::Continue,
        ));
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
                osc_arg_index: Some(0),
                osc_arg_type: Default::default(),
                control_element_type: VirtualControlElementType::Multi,
                ..Default::default()
            }
        );
    }
}
