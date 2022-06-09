use crate::base::eel;
use helgoboss_learn::{Transformation, TransformationInput, TransformationOutput};

use std::sync::Arc;

#[derive(Default)]
pub struct AdditionalTransformationInput {
    pub y_last: f64,
}

#[derive(Debug)]
struct EelUnit {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    // The existence in memory and the Drop is important.
    _vm: eel::Vm,
    _stop: eel::Variable,
    _none: eel::Variable,
    x: eel::Variable,
    y: eel::Variable,
    y_last: eel::Variable,
    rel_time: Option<eel::Variable>,
}

#[derive(Clone, Debug)]
pub enum OutputVariable {
    X,
    Y,
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Clone, Debug)]
pub struct EelTransformation {
    // Arc because EelUnit is not cloneable
    eel_unit: Arc<EelUnit>,
    output_var: OutputVariable,
    wants_to_be_polled: bool,
}

impl EelTransformation {
    // Compiles the given script and creates an appropriate transformation.
    pub fn compile(
        eel_script: &str,
        result_var: OutputVariable,
    ) -> Result<EelTransformation, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let y_last = vm.register_variable("y_last");
        let rel_time_var_name = "rel_time";
        let uses_rel_time = eel_script.contains(rel_time_var_name);
        let rel_time = if uses_rel_time {
            Some(vm.register_variable(rel_time_var_name))
        } else {
            None
        };
        let eel_unit = EelUnit {
            program,
            _stop: vm.register_and_set_variable("stop", STOP),
            _none: vm.register_and_set_variable("none", NONE),
            _vm: vm,
            x,
            y,
            y_last,
            rel_time,
        };
        let transformation = EelTransformation {
            eel_unit: Arc::new(eel_unit),
            output_var: result_var,
            wants_to_be_polled: uses_rel_time,
        };
        Ok(transformation)
    }
}

impl Transformation for EelTransformation {
    type AdditionalInput = AdditionalTransformationInput;

    fn transform(
        &self,
        input: TransformationInput<f64>,
        output_value: f64,
        additional_input: AdditionalTransformationInput,
    ) -> Result<TransformationOutput<f64>, &'static str> {
        let v = unsafe {
            use OutputVariable::*;
            let eel_unit = &*self.eel_unit;
            let (input_var, output_var) = match self.output_var {
                X => (eel_unit.y, eel_unit.x),
                Y => (eel_unit.x, eel_unit.y),
            };
            input_var.set(input.value);
            output_var.set(output_value);
            eel_unit.y_last.set(additional_input.y_last);
            if let Some(rel_time_var) = eel_unit.rel_time {
                rel_time_var.set(input.meta_data.rel_time.as_millis() as _);
            }
            eel_unit.program.execute();
            output_var.get()
        };
        let output = if v == STOP {
            TransformationOutput::Stop
        } else if v == NONE {
            TransformationOutput::None
        } else {
            TransformationOutput::Control(v)
        };
        Ok(output)
    }

    fn wants_to_be_polled(&self) -> bool {
        self.wants_to_be_polled
    }
}

const STOP: f64 = f64::MAX;
const NONE: f64 = f64::MIN;
