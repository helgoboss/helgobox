use crate::base::eel;
use crate::domain::{
    CompartmentParamIndex, CompartmentParams, EffectiveParamValue, ExpressionEvaluator, MappingId,
    RawParamValue, COMPARTMENT_PARAMETER_COUNT, EXPRESSION_NONE_VALUE,
};
use base::regex;
use helgoboss_learn::AbsoluteValue;
use std::collections::HashSet;
use std::error::Error;

#[derive(Debug)]
pub enum ActivationCondition {
    Always,
    Modifiers(Vec<ModifierCondition>),
    Program {
        param_index: CompartmentParamIndex,
        program_index: u32,
    },
    // Boxed in order to keep the enum variants at a similar size (clippy gave that hint)
    Eel(Box<EelCondition>),
    Expression(Box<ExpressionCondition>),
    TargetValue {
        lead_mapping: Option<MappingId>,
        condition: Box<ExpressionEvaluator>,
    },
}

impl ActivationCondition {
    /// Returns if this activation condition can be affected by parameter changes in general.
    pub fn can_be_affected_by_parameters(&self) -> bool {
        !matches!(self, ActivationCondition::Always)
    }

    /// Returns the referenced lead mapping of this activation condition if it's a target-value
    /// based one.
    pub fn target_value_lead_mapping(&self) -> Option<MappingId> {
        match self {
            ActivationCondition::TargetValue {
                lead_mapping: Some(m),
                ..
            } => Some(*m),
            _ => None,
        }
    }

    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    ///
    /// Returns `None` if the condition doesn't depend on parameter values (in which case it must
    /// be evaluated in other ways).
    pub fn is_fulfilled(&self, params: &CompartmentParams) -> Option<bool> {
        use ActivationCondition::*;
        let res = match self {
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
            Expression(condition) => condition.is_fulfilled(params),
            TargetValue { .. } => return None,
        };
        Some(res)
    }

    /// Returns `Some` if the given value update affects the mapping's activation state and if the
    /// resulting state is on or off.
    ///
    /// Passing a `None` target value means the target is inactive.
    pub fn process_target_value_update(
        &self,
        lead_mapping_id: MappingId,
        target_value: Option<AbsoluteValue>,
    ) -> Option<bool> {
        match self {
            ActivationCondition::TargetValue {
                lead_mapping: Some(rm),
                condition,
            } if lead_mapping_id == *rm => {
                let y = match target_value {
                    None => EXPRESSION_NONE_VALUE,
                    Some(v) => v.to_unit_value().get(),
                };
                let result = condition.evaluate_with_vars(|name, _| match name {
                    "none" => Some(EXPRESSION_NONE_VALUE),
                    "y" => Some(y),
                    _ => None,
                });
                result.ok().map(|v| v > 0.0)
            }
            _ => None,
        }
    }

    /// Returns `Some` if the given value update affects the mapping's activation state and if the
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
    pub fn process_param_update(
        &self,
        params: &CompartmentParams,
        // Changed index
        index: CompartmentParamIndex,
        // Previous value at changed index
        previous_value: RawParamValue,
    ) -> Option<bool> {
        use ActivationCondition::*;
        let is_fulfilled = match self {
            Modifiers(conditions) => {
                let is_affected = conditions.iter().any(|c| {
                    c.is_affected_by_param_change(
                        index,
                        previous_value,
                        params.at(index).raw_value(),
                    )
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
                let is_affected = condition
                    .notify_param_changed(index, params.at(index).effective_value().into());
                if !is_affected {
                    return None;
                }
                condition.is_fulfilled()
            }
            Expression(condition) => condition.is_fulfilled(params),
            Always => return None,
            // This conditional activation doesn't depend on parameter values, it's evaluated
            // in other ways.
            TargetValue { .. } => return None,
        };
        Some(is_fulfilled)
    }
}

fn modifier_conditions_are_fulfilled(
    conditions: &[ModifierCondition],
    params: &CompartmentParams,
) -> bool {
    conditions
        .iter()
        .all(|condition| condition.is_fulfilled(params))
}

fn program_condition_is_fulfilled(
    param_index: CompartmentParamIndex,
    program_index: u32,
    params: &CompartmentParams,
) -> bool {
    let current_program_index = match params.at(param_index).effective_value() {
        EffectiveParamValue::Continuous(v) => {
            // If no count given for the parameter, we just assume a count of 100.
            (v * 99.0).round() as u32
        }
        EffectiveParamValue::Discrete(v) => v,
    };
    current_program_index == program_index
}

fn param_value_is_on(value: f32) -> bool {
    value > 0.0
}

#[derive(Debug)]
pub struct ModifierCondition {
    param_index: CompartmentParamIndex,
    is_on: bool,
}

impl ModifierCondition {
    pub fn new(param_index: CompartmentParamIndex, is_on: bool) -> ModifierCondition {
        ModifierCondition { param_index, is_on }
    }

    pub fn is_affected_by_param_change(
        &self,
        index: CompartmentParamIndex,
        previous_value: RawParamValue,
        value: RawParamValue,
    ) -> bool {
        self.param_index == index && param_value_is_on(previous_value) != param_value_is_on(value)
    }

    /// Returns if this activation condition is fulfilled in presence of the given set of
    /// parameters.
    pub fn is_fulfilled(&self, params: &CompartmentParams) -> bool {
        let param_value = params.at(self.param_index).raw_value();
        let is_on = param_value_is_on(param_value);
        is_on == self.is_on
    }
}

#[derive(Debug)]
pub struct ExpressionCondition {
    evaluator: ExpressionEvaluator,
}

impl ExpressionCondition {
    pub fn compile(expression: &str) -> Result<Self, Box<dyn Error>> {
        let condition = Self {
            evaluator: ExpressionEvaluator::compile(expression)?,
        };
        Ok(condition)
    }

    pub fn is_fulfilled(&self, params: &CompartmentParams) -> bool {
        let result = self.evaluator.evaluate_with_params(params);
        result.map(|v| v > 0.0).unwrap_or(false)
    }
}

#[derive(Debug)]
pub struct EelCondition {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    // The existence in memory and the Drop is important.
    _vm: eel::Vm,
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
            _vm: vm,
            params,
            y,
        })
    }

    pub fn notify_params_changed(&self, params: &CompartmentParams) {
        for (i, p) in self.params.iter().enumerate() {
            let i = CompartmentParamIndex::try_from(i as u32).unwrap();
            if let Some(v) = p {
                unsafe {
                    v.set(params.at(i).effective_value().into());
                }
            }
        }
    }

    /// Returns true if activation might have changed.
    pub fn notify_param_changed(&self, param_index: CompartmentParamIndex, value: f64) -> bool {
        if let Some(v) = &self.params[param_index.get() as usize] {
            unsafe {
                v.set(value);
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
        .flat_map(|m| m[1].parse())
        .filter(|i| *i >= 1 && *i <= COMPARTMENT_PARAMETER_COUNT)
        .map(|i: u32| i - 1)
        .collect()
}
