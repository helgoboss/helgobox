use super::convert_source;
use crate::application;
use crate::application::{
    LifecycleMidiMessageModel, LifecycleModel, MappingExtensionModel, RawByteArrayMidiMessage,
};
use crate::domain::Tag;
use crate::infrastructure::api::convert::to_data::glue::convert_glue;
use crate::infrastructure::api::convert::to_data::target::convert_target;
use crate::infrastructure::api::convert::to_data::{
    convert_activation, ApiToDataConversionContext,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::{EnabledData, MappingModelData};
use realearn_api::persistence::*;
use std::convert::TryInto;
use std::str::FromStr;

pub fn convert_mapping(
    m: Mapping,
    conversion_context: &impl ApiToDataConversionContext,
) -> ConversionResult<MappingModelData> {
    let (prevent_echo_feedback, send_feedback_after_control) =
        if let Some(source) = m.source.as_ref() {
            use Source::*;
            let feedback_behavior = match source {
                MidiNoteVelocity(s) => s.feedback_behavior,
                MidiNoteKeyNumber(s) => s.feedback_behavior,
                MidiPolyphonicKeyPressureAmount(s) => s.feedback_behavior,
                MidiControlChangeValue(s) => s.feedback_behavior,
                MidiProgramChangeNumber(s) => s.feedback_behavior,
                MidiChannelPressureAmount(s) => s.feedback_behavior,
                MidiPitchBendChangeValue(s) => s.feedback_behavior,
                MidiParameterNumberValue(s) => s.feedback_behavior,
                MidiRaw(s) => s.feedback_behavior,
                Osc(s) => s.feedback_behavior,
                _ => None,
            };
            match feedback_behavior.unwrap_or_default() {
                FeedbackBehavior::Normal => (false, false),
                FeedbackBehavior::SendFeedbackAfterControl => (false, true),
                FeedbackBehavior::PreventEchoFeedback => (true, false),
            }
        } else {
            (false, false)
        };
    let v = MappingModelData {
        id: m.id.map(|id| id.into()),
        name: m.name.unwrap_or_default(),
        tags: convert_tags(m.tags.unwrap_or_default())?,
        group_id: m.group.map(|g| g.into()).unwrap_or_default(),
        source: convert_source(m.source.unwrap_or_default())?,
        mode: convert_glue(m.glue.unwrap_or_default())?,
        target: convert_target(m.target.unwrap_or_default())?,
        is_enabled: m.enabled.unwrap_or(defaults::MAPPING_ENABLED),
        enabled_data: {
            EnabledData {
                control_is_enabled: m
                    .control_enabled
                    .unwrap_or(defaults::MAPPING_CONTROL_ENABLED),
                feedback_is_enabled: m
                    .feedback_enabled
                    .unwrap_or(defaults::MAPPING_FEEDBACK_ENABLED),
            }
        },
        activation_condition_data: if let Some(cond) = m.activation_condition {
            convert_activation(cond, &|key| conversion_context.param_index_by_key(key))?
        } else {
            Default::default()
        },
        prevent_echo_feedback,
        send_feedback_after_control,
        advanced: convert_advanced(m.on_activate, m.on_deactivate, m.unprocessed)?,
        visible_in_projection: m
            .visible_in_projection
            .unwrap_or(defaults::MAPPING_VISIBLE_IN_PROJECTION),
        success_audio_feedback: m.success_audio_feedback,
    };
    Ok(v)
}

pub fn convert_tags(tag_strings: Vec<String>) -> ConversionResult<Vec<Tag>> {
    tag_strings.into_iter().map(convert_tag).collect()
}

fn convert_tag(tag_string: String) -> ConversionResult<Tag> {
    let tag = Tag::from_str(&tag_string).map_err(anyhow::Error::msg)?;
    Ok(tag)
}

fn convert_advanced(
    on_activate: Option<LifecycleHook>,
    on_deactivate: Option<LifecycleHook>,
    unprocessed: Option<serde_json::Map<String, serde_json::Value>>,
) -> ConversionResult<Option<serde_yaml::mapping::Mapping>> {
    fn into_yaml_mapping(value: serde_yaml::Value) -> serde_yaml::mapping::Mapping {
        if let serde_yaml::Value::Mapping(m) = value {
            m
        } else {
            panic!("must serialize as YAML mapping")
        }
    }
    if on_activate.is_none() && on_deactivate.is_none() && unprocessed.is_none() {
        return Ok(None);
    }
    let extension_model = MappingExtensionModel {
        on_activate: convert_lifecycle_hook(on_activate)?,
        on_deactivate: convert_lifecycle_hook(on_deactivate)?,
    };
    let value = serde_yaml::to_value(&extension_model)?;
    let mut mapping = into_yaml_mapping(value);
    if let Some(u) = unprocessed {
        let unprocessed_value = serde_yaml::to_value(&u)?;
        let unprocessed_mapping = into_yaml_mapping(unprocessed_value);
        for (key, value) in unprocessed_mapping.into_iter() {
            mapping.insert(key, value);
        }
    }
    Ok(Some(mapping))
}

fn convert_lifecycle_hook(hook: Option<LifecycleHook>) -> ConversionResult<LifecycleModel> {
    let v = LifecycleModel {
        send_midi_feedback: {
            let actions: Result<Vec<_>, _> = hook
                .unwrap_or_default()
                .send_midi_feedback
                .unwrap_or_default()
                .into_iter()
                .map(convert_send_midi_feedback_action)
                .collect();
            actions?
        },
    };
    Ok(v)
}

fn convert_send_midi_feedback_action(
    action: SendMidiFeedbackAction,
) -> ConversionResult<LifecycleMidiMessageModel> {
    let v = match action {
        SendMidiFeedbackAction::Raw { message } => {
            LifecycleMidiMessageModel::Raw(convert_raw_midi_message(message)?)
        }
    };
    Ok(v)
}

fn convert_raw_midi_message(msg: RawMidiMessage) -> ConversionResult<application::RawMidiMessage> {
    use application::RawMidiMessage as T;
    let v = match msg {
        RawMidiMessage::HexString(s) => T::HexString(s.try_into()?),
        RawMidiMessage::ByteArray(a) => T::ByteArray(RawByteArrayMidiMessage(a)),
    };
    Ok(v)
}
