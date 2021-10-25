use crate::application::ActivationType;
use crate::domain::GroupId;
use crate::infrastructure::api::convert::from_data::{
    convert_glue, convert_source, convert_target,
};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::ParamRef;
use crate::infrastructure::data::MappingModelData;

pub fn convert_mapping(
    data: MappingModelData,
    group_key_by_id: impl FnOnce(GroupId) -> Option<String>,
) -> ConversionResult<schema::Mapping> {
    let mapping = schema::Mapping {
        key: data.key,
        name: Some(data.name),
        tags: {
            let tags = data.tags.iter().map(|t| t.to_string()).collect();
            Some(tags)
        },
        group: {
            if data.group_id.is_default() {
                None
            } else {
                let group_id = data.group_id;
                let key = group_key_by_id(group_id).unwrap_or_else(|| group_id.to_string());
                Some(key)
            }
        },
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
        feedback_behavior: {
            use schema::FeedbackBehavior as T;
            let v = if data.prevent_echo_feedback {
                // Took precedence if both checkboxes were ticked (was possible in ReaLearn < 2.10.0).
                T::PreventEchoFeedback
            } else if data.send_feedback_after_control {
                T::SendFeedbackAfterControl
            } else {
                T::Normal
            };
            Some(v)
        },
        // TODO-high
        on_activate: None,
        // TODO-high
        on_deactivate: None,
        source: Some(convert_source(data.source)?),
        glue: Some(convert_glue(data.mode)?),
        target: Some(convert_target(data.target)?),
    };
    Ok(mapping)
}
