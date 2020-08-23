use crate::domain::MappingId;
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

#[derive(
    Copy,
    Clone,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum ActivationType {
    #[serde(rename = "always")]
    #[display(fmt = "Always")]
    Always,
    #[serde(rename = "modifiers")]
    #[display(fmt = "When modifiers active")]
    Modifiers,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct ModifierCondition {
    #[serde(rename = "paramIndex")]
    param_index: Option<u32>,
    #[serde(rename = "isOn")]
    is_on: bool,
}

pub fn parameter_value_is_on(value: f32) -> bool {
    value > 0.0
}

impl ModifierCondition {
    pub fn param_index(&self) -> Option<u32> {
        self.param_index
    }

    pub fn with_param_index(&self, param_index: Option<u32>) -> ModifierCondition {
        ModifierCondition {
            param_index,
            ..*self
        }
    }

    pub fn is_on(&self) -> bool {
        self.is_on
    }

    pub fn with_is_on(&self, is_on: bool) -> ModifierCondition {
        ModifierCondition { is_on, ..*self }
    }

    pub fn uses_parameter(&self, param_index: u32) -> bool {
        self.param_index.contains(&param_index)
    }

    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &[f32]) -> bool {
        let param_index = match self.param_index {
            None => return true,
            Some(i) => i,
        };
        let param_value = match params.get(param_index as usize) {
            // Parameter doesn't exist. Shouldn't happen but handle gracefully.
            None => return false,
            Some(v) => v,
        };
        let is_on = parameter_value_is_on(*param_value);
        is_on == self.is_on
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingActivationUpdate {
    pub id: MappingId,
    pub is_active: bool,
}
