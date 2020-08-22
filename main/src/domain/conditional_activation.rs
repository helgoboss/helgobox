use crate::domain::MappingId;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActivationCondition {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "modifiers")]
    Modifiers {
        #[serde(rename = "conditions")]
        conditions: Vec<ModifierCondition>,
    },
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ModifierCondition {
    #[serde(rename = "paramIndex")]
    param_index: u32,
    #[serde(rename = "isOn")]
    is_on: bool,
}

pub fn parameter_value_is_on(value: f32) -> bool {
    value > 0.0
}

impl ActivationCondition {
    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &[f32]) -> bool {
        use ActivationCondition::*;
        match self {
            Always => true,
            Modifiers { conditions } => conditions.iter().all(|condition| {
                let param_value = match params.get(condition.param_index as usize) {
                    // Parameter doesn't exist. Shouldn't happen but handle gracefully.
                    None => return false,
                    Some(v) => v,
                };
                let is_on = parameter_value_is_on(*param_value);
                is_on == condition.is_on
            }),
        }
    }

    /// Returns if this activation condition is affected by the given parameter update.
    pub fn is_affected_by_parameter_update(&self, updated_param_index: u32) -> bool {
        use ActivationCondition::*;
        match self {
            Always => false,
            Modifiers { conditions } => conditions
                .iter()
                .any(|c| c.param_index == updated_param_index),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingActivationUpdate {
    pub id: MappingId,
    pub is_active: bool,
}
