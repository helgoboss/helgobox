use crate::application::{BankConditionModel, ModifierConditionModel};
use crate::domain::CompartmentParamIndex;
use crate::infrastructure::api::convert::ConversionResult;
use crate::infrastructure::data;
use crate::infrastructure::data::{ActivationConditionData, OscValueRange};
use crate::{application, domain};
pub use compartment::*;
use enumflags2::BitFlags;
pub use mapping::*;
use realearn_api::schema::{
    ActivationCondition, Interval, Keystroke, ModifierState, OscArgKind, ParamRef,
    VirtualControlElementCharacter, VirtualControlElementId,
};
use reaper_medium::AcceleratorKeyCode;
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

fn convert_keystroke(s: Keystroke) -> domain::Keystroke {
    domain::Keystroke::new(
        BitFlags::from_bits_truncate(s.modifiers),
        AcceleratorKeyCode::new(s.key),
    )
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

fn convert_osc_value_range(v: Option<Interval<f64>>) -> OscValueRange {
    v.map(|v| {
        let domain_interval = helgoboss_learn::Interval::new_auto(v.0, v.1);
        OscValueRange::from_interval(domain_interval)
    })
    .unwrap_or_default()
}

pub trait ApiToDataConversionContext {
    fn param_index_by_key(&self, key: &str) -> Option<CompartmentParamIndex>;
}

fn convert_activation(
    a: ActivationCondition,
    param_index_by_key: &impl Fn(&str) -> Option<CompartmentParamIndex>,
) -> ConversionResult<ActivationConditionData> {
    use application::ActivationType;
    use ActivationCondition::*;
    let data = match a {
        Modifier(c) => {
            let create_model =
                |state: Option<&ModifierState>| -> ConversionResult<ModifierConditionModel> {
                    let res = if let Some(s) = state {
                        let i = resolve_parameter_ref(&s.parameter, param_index_by_key)?;
                        ModifierConditionModel {
                            param_index: Some(i.try_into()?),
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
        Bank(c) => {
            let i = resolve_parameter_ref(&c.parameter, param_index_by_key)?;
            ActivationConditionData {
                activation_type: ActivationType::Bank,
                program_condition: BankConditionModel {
                    param_index: i.try_into()?,
                    bank_index: c.bank_index,
                },
                ..Default::default()
            }
        }
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
    param_index_by_key: &impl Fn(&str) -> Option<CompartmentParamIndex>,
) -> ConversionResult<CompartmentParamIndex> {
    let res = match param_ref {
        ParamRef::Index(i) => CompartmentParamIndex::try_from(*i)?,
        ParamRef::Key(key) => {
            param_index_by_key(key).ok_or_else(|| format!("Parameter {} not defined", key))?
        }
    };
    Ok(res)
}
