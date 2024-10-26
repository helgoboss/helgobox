use crate::base::eel;
use helgoboss_learn::{
    ControlValue, ControlValueKind, Transformation, TransformationInput, TransformationInstruction,
    TransformationOutput, UnitValue,
};
use std::os::raw::c_void;

use reaper_medium::reaper_str;
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

pub trait Script {
    fn uses_time(&self) -> bool;

    fn evaluate(
        &self,
        input: TransformationInput<UnitValue>,
        output_value: UnitValue,
        additional_input: AdditionalTransformationInput,
    ) -> Result<TransformationOutput<ControlValue>, &'static str>;
}

impl Script for () {
    fn uses_time(&self) -> bool {
        false
    }

    fn evaluate(
        &self,
        input: TransformationInput<UnitValue>,
        output_value: UnitValue,
        additional_input: AdditionalTransformationInput,
    ) -> Result<TransformationOutput<ControlValue>, &'static str> {
        let _ = (input, output_value, additional_input);
        Err("not supported")
    }
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Clone, Debug)]
pub struct EelTransformation {
    // Arc because EelUnit is not cloneable
    eel_unit: Arc<EelUnit>,
    output_var: OutputVariable,
    wants_to_be_polled: bool,
}

impl Script for EelTransformation {
    fn uses_time(&self) -> bool {
        self.wants_to_be_polled()
    }

    fn evaluate(
        &self,
        input: TransformationInput<UnitValue>,
        output_value: UnitValue,
        additional_input: AdditionalTransformationInput,
    ) -> Result<TransformationOutput<ControlValue>, &'static str> {
        self.transform_continuous(input, output_value, additional_input)
    }
}

impl EelTransformation {
    pub fn compile_for_control(eel_script: &str) -> Result<EelTransformation, String> {
        EelTransformation::compile(eel_script, OutputVariable::Y)
    }

    pub fn compile_for_feedback(eel_script: &str) -> Result<EelTransformation, String> {
        EelTransformation::compile(eel_script, OutputVariable::X)
    }

    // Compiles the given script and creates an appropriate transformation.
    fn compile(eel_script: &str, result_var: OutputVariable) -> Result<EelTransformation, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let mut vm = eel::Vm::new();
        vm.register_single_arg_function(reaper_str!("stop"), stop);
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

unsafe extern "C" fn stop(_: *mut c_void, amt: *mut f64) -> f64 {
    CONTROL_AND_STOP_MAGIC + (*amt).clamp(0.0, 1.0)
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
        let (out_val, instruction) = if v == STOP {
            // Stop only
            (None, Some(TransformationInstruction::Stop))
        } else if v == NONE {
            // Neither control nor stop
            (None, None)
        } else if (CONTROL_AND_STOP_MAGIC..=CONTROL_AND_STOP_MAGIC + 1.0).contains(&v) {
            // Both control and stop
            (
                Some(v - CONTROL_AND_STOP_MAGIC),
                Some(TransformationInstruction::Stop),
            )
        } else {
            // Control only
            (Some(v), None)
        };
        let output = TransformationOutput {
            produced_kind: ControlValueKind::AbsoluteContinuous,
            value: out_val,
            instruction,
        };
        Ok(output)
    }

    fn wants_to_be_polled(&self) -> bool {
        self.wants_to_be_polled
    }
}

/// Exposed as variable `stop`.
const STOP: f64 = f64::MAX;
/// Exposed as variable `none`.
const NONE: f64 = f64::MIN;
/// Not exposed but used internally when using function `stop`, e.g. `stop(0.5)`.
///
/// Since all we can do at the moment is returning one number, we define a magic number.
/// If the returned value is at a maximum 1.0 greater than that magic number, we interpret that
/// as stop instruction and extract the corresponding number!
///
/// It's good that this is encapsulated in a function. Maybe we can improve the behavior in future
/// by setting an extra output variable in the implementation of our `stop` function.
const CONTROL_AND_STOP_MAGIC: f64 = 8965019.0;

#[cfg(test)]
mod tests {
    use super::*;
    use bytesize::ByteSize;
    use helgoboss_learn::TransformationInputMetaData;
    use sysinfo::ProcessRefreshKind;

    #[test]
    fn memory_usage() {
        let mut system = sysinfo::System::new();
        let current_pid = sysinfo::get_current_pid().unwrap();
        system.refresh_process_specifics(current_pid, ProcessRefreshKind::new().with_memory());
        let mut last_memory = 0;
        let mut print_mem = move || {
            system.refresh_process_specifics(current_pid, ProcessRefreshKind::new().with_memory());
            let process = system.process(current_pid).unwrap();
            let memory = process.memory();
            let diff = memory as i64 - last_memory as i64;
            let suffix = if diff.is_negative() { "-" } else { "+" };
            println!(
                "Memory changed by {suffix}{}. Total memory usage so far: {} bytes",
                ByteSize::b(diff.unsigned_abs()),
                ByteSize::b(memory)
            );
            last_memory = memory;
        };
        let mut total_count = 0;
        let mut create_transformations = |count| {
            total_count += count;
            let transformations = create_transformations(count);
            println!("Created {count} more transformation units. Total amount of units created so far: {total_count}");
            print_mem();
            transformations
        };
        let mut transformation_containers = vec![
            create_transformations(1),
            create_transformations(1),
            create_transformations(1),
            create_transformations(1),
            create_transformations(1),
            create_transformations(1),
            create_transformations(1),
            create_transformations(100),
        ];
        println!("Now dropping from last to first...");
        while transformation_containers.pop().is_some() {
            println!("Dropped one set of transformations");
            print_mem();
        }
        println!("No transformation sets left");
        print_mem();
    }

    fn create_transformations(count: usize) -> Vec<EelTransformation> {
        (0..count)
            .map(|i| {
                let code = format!("y = x * {i}");
                let transformation = EelTransformation::compile_for_control(&code).unwrap();
                let input = TransformationInput::new(
                    0.5,
                    TransformationInputMetaData {
                        rel_time: Default::default(),
                    },
                );
                transformation
                    .transform(input, 0.5, AdditionalTransformationInput::default())
                    .unwrap();
                transformation
            })
            .collect()
    }
}
