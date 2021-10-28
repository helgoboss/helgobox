mod source;
pub use source::*;
mod glue;
pub use glue::*;
mod group;
pub use group::*;
mod parameter;
pub use parameter::*;
mod mapping;
pub use mapping::*;
mod compartment;
pub use compartment::*;
mod target;

use crate::application::{ActivationType, VirtualControlElementType};
use crate::domain::{GroupId, Tag};
use crate::infrastructure::api::schema;
use crate::infrastructure::api::schema::ParamRef;
use crate::infrastructure::data::{ActivationConditionData, VirtualControlElementIdData};
use helgoboss_learn::OscTypeTag;
pub use target::*;

fn convert_control_element_id(v: VirtualControlElementIdData) -> schema::VirtualControlElementId {
    use schema::VirtualControlElementId as T;
    use VirtualControlElementIdData::*;
    match v {
        Indexed(i) => T::Indexed(i),
        Named(n) => T::Named(n),
    }
}

fn convert_control_element_kind(
    v: VirtualControlElementType,
) -> Option<schema::VirtualControlElementKind> {
    use schema::VirtualControlElementKind as T;
    use VirtualControlElementType::*;
    let res = match v {
        Multi => T::Multi,
        Button => T::Button,
    };
    Some(res)
}

fn convert_osc_argument(
    arg_index: Option<u32>,
    arg_type: OscTypeTag,
) -> Option<schema::OscArgument> {
    let arg_index = arg_index?;
    let arg = schema::OscArgument {
        index: Some(arg_index),
        kind: Some(convert_osc_arg_kind(arg_type)),
    };
    Some(arg)
}

fn convert_osc_arg_kind(v: OscTypeTag) -> schema::OscArgKind {
    use schema::OscArgKind as T;
    use OscTypeTag::*;
    match v {
        Float => T::Float,
        Double => T::Double,
        Bool => T::Bool,
        Nil => T::Nil,
        Inf => T::Inf,
        Int => T::Int,
        String => T::String,
        Blob => T::Blob,
        Time => T::Time,
        Long => T::Long,
        Char => T::Char,
        Color => T::Color,
        Midi => T::Midi,
        Array => T::Array,
    }
}

fn convert_tags(tags: &Vec<Tag>) -> Option<Vec<String>> {
    let tags = tags.iter().map(|t| t.to_string()).collect();
    Some(tags)
}

fn convert_group_id(
    group_id: GroupId,
    context: &impl DataToApiConversionContext,
) -> Option<String> {
    {
        if group_id.is_default() {
            None
        } else {
            let key = context
                .group_key_by_id(group_id)
                .unwrap_or_else(|| group_id.to_string());
            Some(key)
        }
    }
}

pub trait DataToApiConversionContext {
    fn group_key_by_id(&self, group_id: GroupId) -> Option<String>;
}

fn convert_activation_condition(
    condition_data: ActivationConditionData,
) -> Option<schema::ActivationCondition> {
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
}
