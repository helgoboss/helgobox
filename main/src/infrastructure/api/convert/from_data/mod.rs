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
use crate::infrastructure::data::{
    ActivationConditionData, OscValueRange, VirtualControlElementIdData,
};
use helgoboss_learn::OscTypeTag;
use realearn_api::persistence;
use realearn_api::persistence::ParamRef;
pub use target::*;

fn convert_control_element_id(
    v: VirtualControlElementIdData,
) -> persistence::VirtualControlElementId {
    use persistence::VirtualControlElementId as T;
    use VirtualControlElementIdData::*;
    match v {
        Indexed(i) => T::Indexed(i),
        Named(n) => T::Named(n),
    }
}

fn convert_control_element_kind(
    v: VirtualControlElementType,
    style: ConversionStyle,
) -> Option<persistence::VirtualControlElementCharacter> {
    use persistence::VirtualControlElementCharacter as T;
    use VirtualControlElementType::*;
    let res = match v {
        Multi => T::Multi,
        Button => T::Button,
    };
    style.required_value(res)
}

fn convert_keystroke(v: Keystroke) -> persistence::Keystroke {
    persistence::Keystroke {
        modifiers: v.modifiers().bits(),
        key: v.key_code().get(),
    }
}

fn convert_osc_argument(
    arg_index: Option<u32>,
    arg_type: OscTypeTag,
    value_range: OscValueRange,
    style: ConversionStyle,
) -> Option<persistence::OscArgument> {
    let arg_index = arg_index?;
    let arg = persistence::OscArgument {
        index: Some(arg_index),
        kind: style.required_value(convert_osc_arg_kind(arg_type)),
        value_range: style.required_value(convert_osc_value_range(value_range)),
    };
    style.required_value(arg)
}

fn convert_osc_value_range(v: OscValueRange) -> persistence::Interval<f64> {
    let interval = v.to_interval();
    persistence::Interval(interval.min_val(), interval.max_val())
}

fn convert_osc_arg_kind(v: OscTypeTag) -> persistence::OscArgKind {
    use persistence::OscArgKind as T;
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
            Minimal => value.filter(|v| v != &default_value),
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
) -> Option<persistence::ActivationCondition> {
    use persistence::ActivationCondition as T;
    use ActivationType::*;
    match condition_data.activation_type {
        Always => None,
        Modifiers => {
            let condition = persistence::ModifierActivationCondition {
                modifiers: {
                    let mod_conditions = [
                        condition_data.modifier_condition_1,
                        condition_data.modifier_condition_2,
                    ];
                    let mod_states: Vec<_> = mod_conditions
                        .into_iter()
                        .filter_map(|c| {
                            let state = persistence::ModifierState {
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
            let condition = persistence::BankActivationCondition {
                parameter: ParamRef::Index(condition_data.program_condition.param_index.get()),
                bank_index: condition_data.program_condition.bank_index,
            };
            Some(T::Bank(condition))
        }
        Eel => {
            let condition = persistence::EelActivationCondition {
                condition: condition_data.eel_condition,
            };
            Some(T::Eel(condition))
        }
        Expression => {
            let condition = persistence::ExpressionActivationCondition {
                condition: condition_data.eel_condition,
            };
            Some(T::Expression(condition))
        }
        TargetValue => {
            let condition = persistence::TargetValueActivationCondition {
                mapping: condition_data.mapping_key.map(|key| key.into()),
                condition: condition_data.eel_condition,
            };
            Some(T::TargetValue(condition))
        }
    }
}
