use crate::domain::{CompartmentParamIndex, ModifierCondition};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    Default,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum ActivationType {
    #[default]
    #[serde(rename = "always")]
    #[display(fmt = "Always")]
    Always,
    #[serde(rename = "modifiers")]
    #[display(fmt = "When modifiers on/off")]
    Modifiers,
    #[serde(rename = "program")]
    #[display(fmt = "When bank selected")]
    Bank,
    #[serde(rename = "eel")]
    #[display(fmt = "When EEL met")]
    Eel,
    #[serde(rename = "expression")]
    #[display(fmt = "When expression met")]
    Expression,
    #[serde(rename = "target-value")]
    #[display(fmt = "When target value met")]
    TargetValue,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct ModifierConditionModel {
    #[serde(rename = "paramIndex")]
    pub param_index: Option<CompartmentParamIndex>,
    #[serde(rename = "isOn")]
    pub is_on: bool,
}

impl ModifierConditionModel {
    pub fn create_modifier_condition(&self) -> Option<ModifierCondition> {
        self.param_index
            .map(|i| ModifierCondition::new(i, self.is_on))
    }

    pub fn param_index(&self) -> Option<CompartmentParamIndex> {
        self.param_index
    }

    pub fn with_param_index(
        &self,
        param_index: Option<CompartmentParamIndex>,
    ) -> ModifierConditionModel {
        ModifierConditionModel {
            param_index,
            ..*self
        }
    }

    pub fn is_on(&self) -> bool {
        self.is_on
    }

    pub fn with_is_on(&self, is_on: bool) -> ModifierConditionModel {
        ModifierConditionModel { is_on, ..*self }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct BankConditionModel {
    #[serde(rename = "paramIndex")]
    pub param_index: CompartmentParamIndex,
    #[serde(rename = "programIndex")]
    pub bank_index: u32,
}

impl BankConditionModel {
    pub fn param_index(&self) -> CompartmentParamIndex {
        self.param_index
    }

    pub fn with_param_index(&self, param_index: CompartmentParamIndex) -> BankConditionModel {
        BankConditionModel {
            param_index,
            ..*self
        }
    }

    pub fn bank_index(&self) -> u32 {
        self.bank_index
    }

    pub fn with_bank_index(&self, bank_index: u32) -> BankConditionModel {
        BankConditionModel {
            bank_index,
            ..*self
        }
    }
}
