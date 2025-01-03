use crate::base::eel;
use helgoboss_learn::{
    ControlValueKind, Transformation, TransformationInput, TransformationInstruction,
    TransformationOutput,
};
use std::os::raw::c_void;

use atomic::Atomic;
use reaper_medium::reaper_str;
use std::sync::atomic::Ordering;
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
    y_type: eel::Variable,
    last_feedback_value: eel::Variable,
    timestamp: eel::Variable,
    rel_time: Option<eel::Variable>,
}

#[derive(Clone, Debug)]
pub enum OutputVariable {
    X,
    Y,
}

pub trait Script {
    fn uses_time(&self) -> bool;

    fn produces_relative_values(&self) -> bool;

    fn evaluate(
        &self,
        input: TransformationInput<AdditionalTransformationInput>,
    ) -> Result<TransformationOutput, &'static str>;
}

impl Script for () {
    fn uses_time(&self) -> bool {
        false
    }

    fn produces_relative_values(&self) -> bool {
        false
    }

    fn evaluate(
        &self,
        input: TransformationInput<AdditionalTransformationInput>,
    ) -> Result<TransformationOutput, &'static str> {
        let _ = input;
        Err("not supported")
    }
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Clone, Debug)]
pub struct EelTransformation {
    // Arc because EelUnit is not cloneable
    eel_unit: Arc<EelUnit>,
    shared_last_feedback_value: Arc<Atomic<f64>>,
    output_var: OutputVariable,
    wants_to_be_polled: bool,
}

impl Script for EelTransformation {
    fn uses_time(&self) -> bool {
        self.wants_to_be_polled()
    }

    fn produces_relative_values(&self) -> bool {
        let input = TransformationInput::default();
        let Ok(output) = self.transform(input) else {
            return false;
        };
        // For now, we only support relative-discrete
        output.produced_kind == ControlValueKind::RelativeDiscrete
    }

    fn evaluate(
        &self,
        input: TransformationInput<AdditionalTransformationInput>,
    ) -> Result<TransformationOutput, &'static str> {
        self.transform(input)
    }
}

impl EelTransformation {
    pub fn compile_for_control(eel_script: &str) -> Result<EelTransformation, String> {
        EelTransformation::compile(eel_script, OutputVariable::Y)
    }

    pub fn compile_for_feedback(eel_script: &str) -> Result<EelTransformation, String> {
        EelTransformation::compile(eel_script, OutputVariable::X)
    }

    pub fn set_last_feedback_value(&self, value: f64) {
        self.shared_last_feedback_value
            .store(value, Ordering::SeqCst);
    }

    // Compiles the given script and creates an appropriate transformation.
    fn compile(eel_script: &str, result_var: OutputVariable) -> Result<EelTransformation, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let mut vm = eel::Vm::new();
        vm.register_single_arg_function(reaper_str!("stop"), stop);
        vm.register_void_or_bool_function(reaper_str!("realearn_dbg"), realearn_dbg);
        let program = vm.compile(eel_script)?;
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let y_last = vm.register_variable("y_last");
        let y_type = vm.register_variable("y_type");
        let last_feedback_value = vm.register_variable("realearn_last_feedback_value");
        let rel_time_var_name = "rel_time";
        let uses_rel_time = eel_script.contains(rel_time_var_name);
        let timestamp = vm.register_variable("realearn_timestamp");
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
            y_type,
            last_feedback_value,
            timestamp,
            rel_time,
        };
        let transformation = EelTransformation {
            eel_unit: Arc::new(eel_unit),
            shared_last_feedback_value: Arc::new(Atomic::new(-1.0)),
            output_var: result_var,
            wants_to_be_polled: uses_rel_time,
        };
        Ok(transformation)
    }
}

unsafe extern "C" fn stop(_: *mut c_void, amt: *mut f64) -> f64 {
    CONTROL_AND_STOP_MAGIC + (*amt).clamp(0.0, 1.0)
}

unsafe extern "C" fn realearn_dbg(_: *mut c_void, amt: *mut f64) -> bool {
    println!("{}", *amt);
    true
}

impl Transformation for EelTransformation {
    type AdditionalInput = AdditionalTransformationInput;

    fn transform(
        &self,
        input: TransformationInput<Self::AdditionalInput>,
    ) -> Result<TransformationOutput, &'static str> {
        let (raw_output, raw_output_type) = unsafe {
            use OutputVariable::*;
            let eel_unit = &*self.eel_unit;
            let (input_var, output_var) = match self.output_var {
                X => (eel_unit.y, eel_unit.x),
                Y => (eel_unit.x, eel_unit.y),
            };
            input_var.set(input.event.input_value);
            output_var.set(input.context.output_value);
            eel_unit
                .last_feedback_value
                .set(self.shared_last_feedback_value.load(Ordering::SeqCst));
            eel_unit.y_last.set(input.additional_input.y_last);
            eel_unit.timestamp.set(input.event.timestamp.as_secs_f64());
            if let Some(rel_time_var) = eel_unit.rel_time {
                rel_time_var.set(input.context.rel_time.as_millis() as _);
            }
            eel_unit.program.execute();
            (output_var.get(), self.eel_unit.y_type.get())
        };
        let (out_val, instruction) = if raw_output == STOP {
            // Stop only
            (None, Some(TransformationInstruction::Stop))
        } else if raw_output == NONE {
            // Neither control nor stop
            (None, None)
        } else if (CONTROL_AND_STOP_MAGIC..=CONTROL_AND_STOP_MAGIC + 1.0).contains(&raw_output) {
            // Both control and stop
            (
                Some(raw_output - CONTROL_AND_STOP_MAGIC),
                Some(TransformationInstruction::Stop),
            )
        } else {
            // Control only
            (Some(raw_output), None)
        };
        let raw_output_type = raw_output_type.round() as u8;
        let produced_kind = ControlValueKind::try_from(raw_output_type).unwrap_or_default();
        let output = TransformationOutput {
            produced_kind,
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
    use helgoboss_learn::TransformationInputEvent;
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
                let input = TransformationInput {
                    event: TransformationInputEvent {
                        input_value: 0.5,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                transformation.transform(input).unwrap();
                transformation
            })
            .collect()
    }
}
