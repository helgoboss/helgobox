use super::convert_source;
use crate::application;
use crate::application::{BankConditionModel, ModifierConditionModel};
use crate::domain::{GroupId, MappingId, Tag};
use crate::infrastructure::api::convert::to_data::convert_group_key;
use crate::infrastructure::api::convert::to_data::glue::convert_glue;
use crate::infrastructure::api::convert::to_data::target::convert_target;
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::api::schema::*;
use crate::infrastructure::data::{ActivationConditionData, EnabledData, MappingModelData};
use std::str::FromStr;

pub fn convert_mapping(
    m: Mapping,
    group_id_by_key: impl Fn(&str) -> Option<GroupId> + Copy,
    param_index_by_key: &impl Fn(&str) -> Option<u32>,
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
        id: Some(MappingId::random()),
        key: m.key,
        name: m.name.unwrap_or_default(),
        tags: convert_tags(m.tags.unwrap_or_default())?,
        group_id: convert_group_key(m.group, group_id_by_key)?,
        source: convert_source(m.source.unwrap_or_default())?,
        mode: convert_glue(m.glue.unwrap_or_default())?,
        target: convert_target(m.target.unwrap_or_default(), group_id_by_key)?,
        is_enabled: m.enabled.unwrap_or(true),
        enabled_data: {
            EnabledData {
                control_is_enabled: m.control_enabled.unwrap_or(true),
                feedback_is_enabled: m.feedback_enabled.unwrap_or(true),
            }
        },
        activation_condition_data: if let Some(cond) = m.activation_condition {
            convert_activation(cond, param_index_by_key)?
        } else {
            Default::default()
        },
        prevent_echo_feedback,
        send_feedback_after_control,
        advanced: convert_advanced(m.on_activate, m.on_deactivate),
        visible_in_projection: m.visible_in_projection.unwrap_or(true),
    };
    Ok(v)
}

pub fn convert_tags(tag_strings: Vec<String>) -> ConversionResult<Vec<Tag>> {
    tag_strings.into_iter().map(convert_tag).collect()
}

fn convert_tag(tag_string: String) -> ConversionResult<Tag> {
    let tag = Tag::from_str(&tag_string)?;
    Ok(tag)
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

pub fn convert_activation(
    a: ActivationCondition,
    param_by_key: &impl Fn(&str) -> Option<u32>,
) -> ConversionResult<ActivationConditionData> {
    use application::ActivationType;
    use ActivationCondition::*;
    let data = match a {
        Modifier(c) => {
            let create_model =
                |state: Option<&ModifierState>| -> ConversionResult<ModifierConditionModel> {
                    let res = if let Some(s) = state {
                        ModifierConditionModel {
                            param_index: Some(resolve_parameter_ref(&s.parameter, param_by_key)?),
                            is_on: s.on,
                        }
                    } else {
                        Default::default()
                    };
                    Ok(res)
                };
            ActivationConditionData {
                activation_type: ActivationType::Modifiers,
                modifier_condition_1: create_model(c.modifiers.get(0))?,
                modifier_condition_2: create_model(c.modifiers.get(1))?,
                ..Default::default()
            }
        }
        Bank(c) => ActivationConditionData {
            activation_type: ActivationType::Bank,
            program_condition: BankConditionModel {
                param_index: resolve_parameter_ref(&c.parameter, param_by_key)?,
                bank_index: c.bank_index,
            },
            ..Default::default()
        },
        Eel(c) => ActivationConditionData {
            activation_type: ActivationType::Eel,
            eel_condition: c.condition,
            ..Default::default()
        },
    };
    Ok(data)
}

fn resolve_parameter_ref(
    param_ref: &ParamRef,
    param_by_key: impl FnOnce(&str) -> Option<u32>,
) -> ConversionResult<u32> {
    let res = match param_ref {
        ParamRef::Index(i) => *i,
        ParamRef::Key(key) => {
            param_by_key(&key).ok_or_else(|| format!("Parameter {} not defined", key))?
        }
    };
    Ok(res)
}
