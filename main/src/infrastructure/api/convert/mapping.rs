use super::convert_source;
use crate::domain::{GroupId, MappingId, Tag};
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{ActivationConditionData, EnabledData, MappingModelData};
use std::error::Error;
use std::str::FromStr;

pub fn convert_mapping(
    m: Mapping,
    group_by_key: impl Fn(&str) -> Option<GroupId>,
) -> Result<MappingModelData, Box<dyn Error>> {
    let v = MappingModelData {
        id: Some(MappingId::random()),
        name: m.name.unwrap_or_default(),
        tags: {
            let res: Result<Vec<_>, _> = m
                .tags
                .unwrap_or_default()
                .into_iter()
                .map(|t| Tag::from_str(&t))
                .collect();
            res?
        },
        group_id: {
            if let Some(key) = m.group {
                group_by_key(&key).ok_or_else(|| format!("Group {} not defined", key))?
            } else {
                GroupId::default()
            }
        },
        source: convert_source(m.source.unwrap_or_default())?,
        mode: todo!(),
        target: todo!(),
        is_enabled: m.enabled.unwrap_or(true),
        enabled_data: convert_enabled_data(&m),
        activation_condition_data: convert_activation(m.active),
        prevent_echo_feedback: m.feedback_behavior == Some(FeedbackBehavior::PreventEchoFeedback),
        send_feedback_after_control: m.feedback_behavior
            == Some(FeedbackBehavior::SendFeedbackAfterControl),
        advanced: convert_advanced(m.on_activate, m.on_deactivate),
        visible_in_projection: m.visible_in_projection.unwrap_or(true),
    };
    Ok(v)
}

fn convert_advanced(
    on_activate: Option<Lifecycle>,
    on_deactivate: Option<Lifecycle>,
) -> Option<serde_yaml::mapping::Mapping> {
    if on_activate.is_none() && on_deactivate.is_none() {
        return None;
    }
    // TODO-high
    None
}

fn convert_activation(a: Option<Active>) -> ActivationConditionData {
    // TODO-high
    ActivationConditionData {
        activation_type: Default::default(),
        modifier_condition_1: Default::default(),
        modifier_condition_2: Default::default(),
        program_condition: Default::default(),
        eel_condition: "".to_string(),
    }
}

fn convert_enabled_data(m: &Mapping) -> EnabledData {
    EnabledData {
        control_is_enabled: m.control_enabled.unwrap_or(true),
        feedback_is_enabled: m.feedback_enabled.unwrap_or(true),
    }
}
