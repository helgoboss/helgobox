use crate::core::eel;
use helgoboss_learn::{Transformation, UnitValue};
use std::convert::TryInto;
use std::ffi::CString;
use std::os::raw::c_void;
use std::rc::Rc;

#[derive(Debug)]
struct EelUnit {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    x: eel::Variable,
    y: eel::Variable,
}

#[derive(Clone, Debug)]
pub enum ResultVariable {
    X,
    Y,
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Clone, Debug)]
pub struct EelTransformation {
    // Rc because EelUnit is not cloneable
    eel_unit: Rc<EelUnit>,
    result_var: ResultVariable,
}

impl EelTransformation {
    // Compiles the given script and creates an appropriate transformation.
    pub fn compile(
        eel_script: &str,
        result_var: ResultVariable,
    ) -> Result<EelTransformation, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let eel_unit = EelUnit { vm, program, x, y };
        Ok(EelTransformation {
            eel_unit: Rc::new(eel_unit),
            result_var,
        })
    }
}

impl Transformation for EelTransformation {
    fn transform(&self, input_value: UnitValue) -> Result<UnitValue, ()> {
        let result = unsafe {
            self.eel_unit.x.set(input_value.get());
            self.eel_unit.y.set(input_value.get());
            self.eel_unit.program.execute();
            use ResultVariable::*;
            match self.result_var {
                X => self.eel_unit.x.get(),
                Y => self.eel_unit.y.get(),
            }
        };
        result.try_into().map_err(|_| ())
    }
}
