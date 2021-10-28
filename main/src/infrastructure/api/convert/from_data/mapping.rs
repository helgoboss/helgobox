use crate::application::{
    ActivationType, LifecycleMidiMessageModel, LifecycleModel, MappingExtensionModel,
    RawMidiMessage,
};
use crate::domain::GroupId;
use crate::infrastructure::api::convert::from_data::{
    convert_glue, convert_group_id, convert_source, convert_tags, convert_target, NewSourceProps,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::{LifecycleHook, ParamRef};
use crate::infrastructure::data::MappingModelData;

pub fn convert_mapping(
    data: MappingModelData,
    group_key_by_id: impl Fn(GroupId) -> Option<String> + Copy,
) -> ConversionResult<schema::Mapping> {
    let advanced = convert_advanced(data.advanced)?;
    let mapping = schema::Mapping {
        key: data.key,
        name: Some(data.name),
        tags: convert_tags(&data.tags),
        group: convert_group_id(data.group_id, group_key_by_id),
        visible_in_projection: Some(data.visible_in_projection),
        enabled: Some(data.is_enabled),
        control_enabled: Some(data.enabled_data.control_is_enabled),
        feedback_enabled: Some(data.enabled_data.feedback_is_enabled),
        activation_condition: {
            let condition_data = data.activation_condition_data;
            use schema::ActivationCondition as T;
            use ActivationType::*;
            match condition_data.activation_type {
                Always => None,
                Modifiers => Some(T::Modifier(schema::ModifierActivationCondition {
                    modifiers: IntoIterator::into_iter([
                        condition_data.modifier_condition_1,
                        condition_data.modifier_condition_2,
                    ])
                    .filter_map(|c| {
                        let state = schema::ModifierState {
                            parameter: ParamRef::Index(c.param_index?),
                            on: c.is_on,
                        };
                        Some(state)
                    })
                    .collect(),
                })),
                Bank => Some(T::Bank(schema::BankActivationCondition {
                    parameter: ParamRef::Index(condition_data.program_condition.param_index),
                    bank_index: condition_data.program_condition.bank_index,
                })),
                Eel => Some(T::Eel(schema::EelActivationCondition {
                    condition: condition_data.eel_condition,
                })),
            }
        },
        on_activate: advanced.extension_desc.on_activate,
        on_deactivate: advanced.extension_desc.on_deactivate,
        source: {
            let new_source_props = NewSourceProps {
                prevent_echo_feedback: data.prevent_echo_feedback,
                send_feedback_after_control: data.send_feedback_after_control,
            };
            Some(convert_source(data.source, new_source_props)?)
        },
        glue: Some(convert_glue(data.mode)?),
        target: Some(convert_target(data.target, group_key_by_id)?),
        unprocessed: advanced.unprocessed,
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
    on_activate: Option<schema::LifecycleHook>,
    on_deactivate: Option<schema::LifecycleHook>,
}

fn convert_advanced(
    advanced: Option<serde_yaml::mapping::Mapping>,
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
            convert_extension_model(extension_model)?
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
) -> ConversionResult<ExtensionDesc> {
    let desc = ExtensionDesc {
        on_activate: convert_lifecycle_model(extension_model.on_activate)?,
        on_deactivate: convert_lifecycle_model(extension_model.on_deactivate)?,
    };
    Ok(desc)
}

fn convert_lifecycle_model(
    lifecycle_model: LifecycleModel,
) -> ConversionResult<Option<schema::LifecycleHook>> {
    let hook = LifecycleHook {
        send_midi_feedback: {
            let actions: Result<Vec<_>, _> = lifecycle_model
                .send_midi_feedback
                .into_iter()
                .map(convert_lifecycle_midi_message_model)
                .collect();
            Some(actions?)
        },
    };
    Ok(Some(hook))
}

fn convert_lifecycle_midi_message_model(
    model: LifecycleMidiMessageModel,
) -> ConversionResult<schema::SendMidiFeedbackAction> {
    let action = match model {
        LifecycleMidiMessageModel::Raw(msg) => {
            let message = convert_raw_midi_msg(msg)?;
            schema::SendMidiFeedbackAction::Raw { message }
        }
    };
    Ok(action)
}

fn convert_raw_midi_msg(msg: RawMidiMessage) -> ConversionResult<schema::RawMidiMessage> {
    use schema::RawMidiMessage as T;
    let v = match msg {
        RawMidiMessage::HexString(s) => T::HexString(s.to_string()),
        RawMidiMessage::ByteArray(a) => T::ByteArray(a.0),
    };
    Ok(v)
}
