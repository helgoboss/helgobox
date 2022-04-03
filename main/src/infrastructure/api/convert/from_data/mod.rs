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
use crate::domain::{Keystroke, Tag};
use crate::infrastructure::api::convert::defaults;
use crate::infrastructure::data::{ActivationConditionData, VirtualControlElementIdData};
use helgoboss_learn::OscTypeTag;
use realearn_api::schema;
use realearn_api::schema::ParamRef;
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
    style: ConversionStyle,
) -> Option<schema::VirtualControlElementCharacter> {
    use schema::VirtualControlElementCharacter as T;
    use VirtualControlElementType::*;
    let res = match v {
        Multi => T::Multi,
        Button => T::Button,
    };
    style.required_value(res)
}

fn convert_keystroke(v: Keystroke) -> schema::Keystroke {
    schema::Keystroke {
        modifiers: v.modifiers().bits(),
        key: v.key().get(),
    }
}

fn convert_osc_argument(
    arg_index: Option<u32>,
    arg_type: OscTypeTag,
    style: ConversionStyle,
) -> Option<schema::OscArgument> {
    let arg_index = arg_index?;
    let arg = schema::OscArgument {
        index: style.required_value_with_default(arg_index, defaults::OSC_ARG_INDEX),
        kind: style.required_value(convert_osc_arg_kind(arg_type)),
    };
    style.required_value(arg)
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

fn convert_tags(tags: &[Tag], style: ConversionStyle) -> Option<Vec<String>> {
    let tags = tags.iter().map(|t| t.to_string()).collect();
    style.required_value(tags)
}

#[derive(Copy, Clone)]
pub enum ConversionStyle {
    Minimal,
    IncludeDefaultValues,
}

impl ConversionStyle {
    pub fn required_value<T: PartialEq + Default>(&self, value: T) -> Option<T> {
        self.required_value_with_default(value, T::default())
    }

    pub fn required_value_with_default<T: PartialEq>(
        &self,
        value: T,
        default_value: T,
    ) -> Option<T> {
        use ConversionStyle::*;
        match self {
            Minimal => {
                if value == default_value {
                    None
                } else {
                    Some(value)
                }
            }
            IncludeDefaultValues => Some(value),
        }
    }

    pub fn optional_value<T: PartialEq + Default>(&self, value: Option<T>) -> Option<T> {
        self.optional_value_with_default(value, T::default())
    }

    pub fn optional_value_with_default<T: PartialEq>(
        &self,
        value: Option<T>,
        default_value: T,
    ) -> Option<T> {
        use ConversionStyle::*;
        match self {
            Minimal => {
                if let Some(v) = value {
                    if v == default_value {
                        None
                    } else {
                        Some(v)
                    }
                } else {
                    None
                }
            }
            IncludeDefaultValues => {
                if value.is_some() {
                    value
                } else {
                    Some(default_value)
                }
            }
        }
    }
}

fn convert_activation_condition(
    condition_data: ActivationConditionData,
) -> Option<schema::ActivationCondition> {
    use schema::ActivationCondition as T;
    use ActivationType::*;
    match condition_data.activation_type {
        Always => None,
        Modifiers => {
            let condition = schema::ModifierActivationCondition {
                modifiers: {
                    let mod_conditions = [
                        condition_data.modifier_condition_1,
                        condition_data.modifier_condition_2,
                    ];
                    let mod_states: Vec<_> = mod_conditions
                        .into_iter()
                        .filter_map(|c| {
                            let state = schema::ModifierState {
                                parameter: ParamRef::Index(c.param_index?.get()),
                                on: c.is_on,
                            };
                            Some(state)
                        })
                        .collect();
                    Some(mod_states)
                },
            };
            Some(T::Modifier(condition))
        }
        Bank => {
            let condition = schema::BankActivationCondition {
                parameter: ParamRef::Index(condition_data.program_condition.param_index.get()),
                bank_index: condition_data.program_condition.bank_index,
            };
            Some(T::Bank(condition))
        }
        Eel => {
            let condition = schema::EelActivationCondition {
                condition: condition_data.eel_condition,
            };
            Some(T::Eel(condition))
        }
    }
}
