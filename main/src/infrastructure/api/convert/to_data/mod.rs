use crate::application;
use crate::application::{BankConditionModel, ModifierConditionModel};
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::data;
use crate::infrastructure::data::ActivationConditionData;
pub use compartment::*;
pub use mapping::*;
use realearn_api::schema::{
    ActivationCondition, ModifierState, OscArgKind, ParamRef, VirtualControlElementCharacter,
    VirtualControlElementId,
};
use source::*;

mod compartment;
mod glue;
mod group;
mod mapping;
mod parameter;
mod source;
mod target;

fn convert_control_element_type(
    s: VirtualControlElementCharacter,
) -> application::VirtualControlElementType {
    use application::VirtualControlElementType as T;
    use VirtualControlElementCharacter::*;
    match s {
        Multi => T::Multi,
        Button => T::Button,
    }
}

fn convert_control_element_id(s: VirtualControlElementId) -> data::VirtualControlElementIdData {
    use data::VirtualControlElementIdData as T;
    use VirtualControlElementId::*;
    match s {
        Indexed(i) => T::Indexed(i),
        Named(s) => T::Named(s),
    }
}

fn convert_osc_arg_type(s: OscArgKind) -> helgoboss_learn::OscTypeTag {
    use helgoboss_learn::OscTypeTag as T;
    use OscArgKind::*;
    match s {
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

pub trait ApiToDataConversionContext {
    fn param_index_by_key(&self, key: &str) -> Option<u32>;
}

fn convert_activation(
    a: ActivationCondition,
    param_index_by_key: &impl Fn(&str) -> Option<u32>,
) -> ConversionResult<ActivationConditionData> {
    use application::ActivationType;
    use ActivationCondition::*;
    let data = match a {
        Modifier(c) => {
            let create_model =
                |state: Option<&ModifierState>| -> ConversionResult<ModifierConditionModel> {
                    let res = if let Some(s) = state {
                        ModifierConditionModel {
                            param_index: Some(resolve_parameter_ref(
                                &s.parameter,
                                param_index_by_key,
                            )?),
                            is_on: s.on,
                        }
                    } else {
                        Default::default()
                    };
                    Ok(res)
                };
            ActivationConditionData {
                activation_type: ActivationType::Modifiers,
                modifier_condition_1: create_model(
                    c.modifiers.as_ref().map(|m| m.get(0)).unwrap_or_default(),
                )?,
                modifier_condition_2: create_model(
                    c.modifiers.as_ref().map(|m| m.get(1)).unwrap_or_default(),
                )?,
                ..Default::default()
            }
        }
        Bank(c) => ActivationConditionData {
            activation_type: ActivationType::Bank,
            program_condition: BankConditionModel {
                param_index: resolve_parameter_ref(&c.parameter, param_index_by_key)?,
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
    param_index_by_key: &impl Fn(&str) -> Option<u32>,
) -> ConversionResult<u32> {
    let res = match param_ref {
        ParamRef::Index(i) => *i,
        ParamRef::Key(key) => {
            param_index_by_key(key).ok_or_else(|| format!("Parameter {} not defined", key))?
        }
    };
    Ok(res)
}
