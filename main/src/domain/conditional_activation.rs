use crate::core::eel;
use crate::domain::PLUGIN_PARAMETER_COUNT;
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
    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &[f32]) -> bool {
        use ActivationCondition::*;
        match self {
            Always => true,
            Modifiers(conditions) => conditions
                .iter()
                .all(|condition| condition.is_fulfilled(params)),
            Program {
                param_index,
                program_index,
            } => {
                let param_value = params[*param_index as usize];
                let current_program_index = (param_value * 99.0).round() as u32;
                current_program_index == *program_index
            }
            Eel(condition) => condition.is_fulfilled(),
        }
    }

    /// Returns if this activation condition is affected by parameter changes in general.
    pub fn is_affected_by_parameters(&self) -> bool {
        match self {
            ActivationCondition::Always => false,
            _ => true,
        }
    }

    /// Returns if this activation condition is affected by the given parameter update.
    ///
    /// This is a bit hacky because in case of EEL, this actually writes something - but in EEL
    /// world, not in Rust world, so we don't need mutable (which is important in order to avoid
    /// a combination of mutable borrow of mapping and immutable borrow of parameter array
    /// in `MainProcessor`).
    //
    // TODO-low Maybe there's better solution.
    pub fn notify_param_changed(&self, index: u32, previous_value: f32, value: f32) -> bool {
        use ActivationCondition::*;
        match self {
            Always => false,
            Modifiers(conditions) => conditions
                .iter()
                .any(|c| c.is_affected_by_param_change(index, previous_value, value)),
            Program {
                param_index: bank_param_index,
                ..
            } => index == *bank_param_index,
            Eel(condition) => condition.notify_param_changed(index, value),
        }
    }
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
    pub fn is_fulfilled(&self, params: &[f32]) -> bool {
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
    params: [Option<eel::Variable>; PLUGIN_PARAMETER_COUNT as usize],
    y: eel::Variable,
}

impl EelCondition {
    // Compiles the given script and creates an appropriate condition.
    pub fn compile(eel_script: &str, initial_params: &[f32]) -> Result<EelCondition, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let y = vm.register_variable("y");
        let params = {
            let mut array = [None; PLUGIN_PARAMETER_COUNT as usize];
            for i in extract_used_param_indexes(eel_script).into_iter() {
                let variable_name = format!("p{}", i + 1);
                let variable = vm.register_variable(&variable_name);
                // Set initial value so we can calculate the initial activation result after
                // compilation. All subsequent parameter value changes are done incrementally via
                // single parameter updates (which is more efficient).
                unsafe {
                    variable.set(initial_params[i as usize] as f64);
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
        .filter(|i| *i >= 1 && *i <= PLUGIN_PARAMETER_COUNT)
        .map(|i: u32| i - 1)
        .collect()
}
