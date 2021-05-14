use crate::base::eel;
use crate::domain::{ParameterSlice, COMPARTMENT_PARAMETER_COUNT};
use std::collections::HashSet;

#[derive(Debug)]
pub enum ActivationCondition {
    Always,
    Modifiers(Vec<ModifierCondition>),
    Program {
        param_index: u32,
        program_index: u32,
    },
    // Boxed in order to keep the enum variants at a similar size (clippy gave that hint)
    Eel(Box<EelCondition>),
}

impl ActivationCondition {
    /// Returns if this activation condition can be affected by parameter changes in general.
    pub fn can_be_affected_by_parameters(&self) -> bool {
        !matches!(self, ActivationCondition::Always)
    }

    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &ParameterSlice) -> bool {
        use ActivationCondition::*;
        match self {
            Always => true,
            Modifiers(conditions) => modifier_conditions_are_fulfilled(conditions, params),
            Program {
                param_index,
                program_index,
            } => program_condition_is_fulfilled(*param_index, *program_index, params),
            Eel(condition) => {
                condition.notify_params_changed(params);
                condition.is_fulfilled()
            }
        }
    }

    /// Returns `Some` if the given value change affects the mapping's activation state and if the
    /// resulting state is on or off.
    ///
    /// Other parameters in the given array should not have changed! That's especially important
    /// for the EEL activation condition which will ignore the other values in the array for
    /// performance reasons and just look at the difference (because it has the array already
    /// stored in the EEL VM). For performance reasons as well, the other activation condition types
    /// don't store anything and read the given parameter array.
    ///
    /// Attention: For EEL condition, this has a side effect!
    /// TODO-low This is not visible because it's &self.
    pub fn is_fulfilled_single(
        &self,
        params: &ParameterSlice,
        // Changed index
        index: u32,
        // Previous value at changed index
        previous_value: f32,
    ) -> Option<bool> {
        use ActivationCondition::*;
        let is_fulfilled = match self {
            Modifiers(conditions) => {
                let is_affected = conditions.iter().any(|c| {
                    c.is_affected_by_param_change(index, previous_value, params[index as usize])
                });
                if !is_affected {
                    return None;
                }
                modifier_conditions_are_fulfilled(conditions, params)
            }
            Program {
                param_index,
                program_index,
            } => {
                if index != *param_index {
                    return None;
                }
                program_condition_is_fulfilled(*param_index, *program_index, params)
            }
            Eel(condition) => {
                let is_affected = condition.notify_param_changed(index, params[index as usize]);
                if !is_affected {
                    return None;
                }
                condition.is_fulfilled()
            }
            Always => return None,
        };
        Some(is_fulfilled)
    }
}

fn modifier_conditions_are_fulfilled(
    conditions: &[ModifierCondition],
    params: &ParameterSlice,
) -> bool {
    conditions
        .iter()
        .all(|condition| condition.is_fulfilled(params))
}

fn program_condition_is_fulfilled(
    param_index: u32,
    program_index: u32,
    params: &ParameterSlice,
) -> bool {
    let param_value = params[param_index as usize];
    let current_program_index = (param_value * 99.0).round() as u32;
    current_program_index == program_index
}

fn param_value_is_on(value: f32) -> bool {
    value > 0.0
}

#[derive(Debug)]
pub struct ModifierCondition {
    param_index: u32,
    is_on: bool,
}

impl ModifierCondition {
    pub fn new(param_index: u32, is_on: bool) -> ModifierCondition {
        ModifierCondition { param_index, is_on }
    }

    pub fn is_affected_by_param_change(&self, index: u32, previous_value: f32, value: f32) -> bool {
        self.param_index == index && param_value_is_on(previous_value) != param_value_is_on(value)
    }

    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &ParameterSlice) -> bool {
        let param_value = match params.get(self.param_index as usize) {
            // Parameter doesn't exist. Shouldn't happen but handle gracefully.
            None => return false,
            Some(v) => v,
        };
        let is_on = param_value_is_on(*param_value);
        is_on == self.is_on
    }
}

#[derive(Debug)]
pub struct EelCondition {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    params: [Option<eel::Variable>; COMPARTMENT_PARAMETER_COUNT as usize],
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
            let mut array = [None; COMPARTMENT_PARAMETER_COUNT as usize];
            for i in extract_used_param_indexes(eel_script).into_iter() {
                let variable_name = format!("p{}", i + 1);
                let variable = vm.register_variable(&variable_name);
                // Set initial value so we can calculate the initial activation result after
                // compilation. All subsequent parameter value changes are done incrementally via
                // single parameter updates (which is more efficient).
                unsafe {
                    // We initialize this to zero. It will be constantly updated to current values
                    // in main processor.
                    variable.set(0.0);
                }
                array[i as usize] = Some(variable);
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

    pub fn notify_params_changed(&self, params: &ParameterSlice) {
        for (i, p) in self.params.iter().enumerate() {
            if let Some(v) = p {
                unsafe {
                    v.set(params[i] as f64);
                }
            }
        }
    }

    /// Returns true if activation might have changed.
    pub fn notify_param_changed(&self, param_index: u32, value: f32) -> bool {
        if let Some(v) = &self.params[param_index as usize] {
            unsafe {
                v.set(value as f64);
            }
            true
        } else {
            false
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
    let param_regex = regex!(r#"\bp([0-9]+)\b"#);
    param_regex
        .captures_iter(eel_script)
        .map(|m| m[1].parse())
        .flatten()
        .filter(|i| *i >= 1 && *i <= COMPARTMENT_PARAMETER_COUNT)
        .map(|i: u32| i - 1)
        .collect()
}
