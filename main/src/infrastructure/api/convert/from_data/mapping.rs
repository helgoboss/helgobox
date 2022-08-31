use crate::application::{
    LifecycleMidiMessageModel, LifecycleModel, MappingExtensionModel, RawMidiMessage,
};
use crate::infrastructure::api::convert::from_data::{
    convert_activation_condition, convert_glue, convert_source, convert_tags, convert_target,
    ConversionStyle, NewSourceProps,
};
use crate::infrastructure::api::convert::{defaults, ConversionResult};
use crate::infrastructure::data::MappingModelData;
use realearn_api::persistence;
use realearn_api::persistence::LifecycleHook;

pub fn convert_mapping(
    data: MappingModelData,
    style: ConversionStyle,
) -> ConversionResult<persistence::Mapping> {
    let advanced = convert_advanced(data.advanced, style)?;
    let mapping = persistence::Mapping {
        id: style.optional_value(data.id.map(|id| id.into())),
        name: style.required_value(data.name),
        tags: convert_tags(&data.tags, style),
        group: style.required_value(data.group_id.into()),
        visible_in_projection: style.required_value_with_default(
            data.visible_in_projection,
            defaults::MAPPING_VISIBLE_IN_PROJECTION,
        ),
        enabled: style.required_value_with_default(data.is_enabled, defaults::MAPPING_ENABLED),
        control_enabled: style.required_value_with_default(
            data.enabled_data.control_is_enabled,
            defaults::MAPPING_CONTROL_ENABLED,
        ),
        feedback_enabled: style.required_value_with_default(
            data.enabled_data.feedback_is_enabled,
            defaults::MAPPING_FEEDBACK_ENABLED,
        ),
        activation_condition: convert_activation_condition(data.activation_condition_data),
        on_activate: style.optional_value(advanced.extension_desc.on_activate),
        on_deactivate: style.optional_value(advanced.extension_desc.on_deactivate),
        source: {
            let new_source_props = NewSourceProps {
                prevent_echo_feedback: data.prevent_echo_feedback,
                send_feedback_after_control: data.send_feedback_after_control,
            };
            style.required_value(convert_source(data.source, new_source_props, style)?)
        },
        glue: style.required_value(convert_glue(data.mode, style)?),
        target: style.required_value(convert_target(data.target, style)?),
        success_audio_feedback: data.success_audio_feedback,
        unprocessed: style.optional_value(advanced.unprocessed),
    };
    Ok(mapping)
}

#[derive(Default)]
struct AdvancedDesc {
    extension_desc: ExtensionDesc,
    unprocessed: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Default)]
struct ExtensionDesc {
    on_activate: Option<persistence::LifecycleHook>,
    on_deactivate: Option<persistence::LifecycleHook>,
}

fn convert_advanced(
    advanced: Option<serde_yaml::mapping::Mapping>,
    style: ConversionStyle,
) -> ConversionResult<AdvancedDesc> {
    let mut advanced = match advanced {
        None => return Ok(Default::default()),
        Some(a) => a,
    };
    // Move known properties into own YAML mapping
    let mut known_yaml = serde_yaml::mapping::Mapping::new();
    let on_activate_key = serde_yaml::Value::String("on_activate".to_string());
    let on_deactivate_key = serde_yaml::Value::String("on_deactivate".to_string());
    if let Some(on_activate) = advanced.remove(&on_activate_key) {
        known_yaml.insert(on_activate_key, on_activate);
    }
    if let Some(on_deactivate) = advanced.remove(&on_deactivate_key) {
        known_yaml.insert(on_deactivate_key, on_deactivate);
    }
    let desc = AdvancedDesc {
        extension_desc: {
            let extension_model = serde_yaml::from_value(serde_yaml::Value::Mapping(known_yaml))?;
            convert_extension_model(extension_model, style)?
        },
        // Sort out unknown properties as "unprocessed"
        unprocessed: {
            let json_value = serde_json::to_value(advanced)?;
            if let serde_json::Value::Object(map) = json_value {
                Some(map)
            } else {
                panic!("impossible that a YAML mapping is not serialized as JSON object")
            }
        },
    };
    Ok(desc)
}

fn convert_extension_model(
    extension_model: MappingExtensionModel,
    style: ConversionStyle,
) -> ConversionResult<ExtensionDesc> {
    let desc = ExtensionDesc {
        on_activate: convert_lifecycle_model(extension_model.on_activate, style)?,
        on_deactivate: convert_lifecycle_model(extension_model.on_deactivate, style)?,
    };
    Ok(desc)
}

fn convert_lifecycle_model(
    lifecycle_model: LifecycleModel,
    style: ConversionStyle,
) -> ConversionResult<Option<persistence::LifecycleHook>> {
    let hook = LifecycleHook {
        send_midi_feedback: {
            let actions: Result<Vec<_>, _> = lifecycle_model
                .send_midi_feedback
                .into_iter()
                .map(convert_lifecycle_midi_message_model)
                .collect();
            style.required_value(actions?)
        },
    };
    Ok(style.required_value(hook))
}

fn convert_lifecycle_midi_message_model(
    model: LifecycleMidiMessageModel,
) -> ConversionResult<persistence::SendMidiFeedbackAction> {
    let action = match model {
        LifecycleMidiMessageModel::Raw(msg) => {
            let message = convert_raw_midi_msg(msg)?;
            persistence::SendMidiFeedbackAction::Raw { message }
        }
    };
    Ok(action)
}

fn convert_raw_midi_msg(msg: RawMidiMessage) -> ConversionResult<persistence::RawMidiMessage> {
    use persistence::RawMidiMessage as T;
    let v = match msg {
        RawMidiMessage::HexString(s) => T::HexString(s.to_string()),
        RawMidiMessage::ByteArray(a) => T::ByteArray(a.0),
    };
    Ok(v)
}
