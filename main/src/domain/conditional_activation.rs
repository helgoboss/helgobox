use crate::core::eel;
use crate::domain::{MappingId, PLUGIN_PARAMETER_COUNT};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use regex::Captures;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    #[serde(rename = "eel")]
    #[display(fmt = "EEL")]
    Eel,
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

#[derive(Debug)]
struct EelCondition {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    params: [Option<eel::Variable>; PLUGIN_PARAMETER_COUNT as usize],
    y: eel::Variable,
}

impl EelCondition {
    // Compiles the given script and creates an appropriate condition.
    pub fn compile(eel_script: &str) -> Result<EelCondition, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let y = vm.register_variable("y");
        let params = {
            let mut array = [None; PLUGIN_PARAMETER_COUNT as usize];
            for i in extract_used_param_indexes(eel_script).into_iter() {
                array[i as usize] = Some(vm.register_variable(&format!("p{}", i)));
            }
            array
        };
        Ok(EelCondition {
            program,
            vm,
            params,
            y,
        })
    }

    /// Returns true if activation might have changed.
    pub fn notify_param_changed(&mut self, param_index: u32, value: f32) -> bool {
        if let Some(v) = &mut self.params[param_index as usize] {
            unsafe {
                v.set(value as f64);
            }
            true
        } else {
            false
        }
    }

    pub fn set_params(&mut self, params: &[f32]) {
        for i in (0..PLUGIN_PARAMETER_COUNT) {
            self.notify_param_changed(i, params[i as usize]);
        }
    }

    pub fn is_fulfilled(&self) -> bool {
        let result = unsafe {
            self.program.execute();
            self.y.get()
        };
        result > 0.0
    }
}

fn extract_used_param_indexes(eel_script: &str) -> HashSet<u32> {
    let param_regex = regex!(r#"\Wp([0-9]+)\W"#);
    param_regex
        .find_iter(eel_script)
        .map(|m| m.as_str().parse())
        .flatten()
        .filter(|i| *i >= 1 && *i <= PLUGIN_PARAMETER_COUNT)
        .collect()
}
