use crate::api::ExpressionEvaluator;
use eel2::{Program, Variable, Vm};
use std::error::Error;

pub struct Eel2ExpressionEvaluator {
    // Declared above VM in order to be dropped before VM is dropped.
    program: Program,
    // The existence in memory and the Drop is important.
    _vm: Vm,
    x: Variable,
    y: Variable,
}

impl Eel2ExpressionEvaluator {
    pub fn compile(expression: &str) -> Result<Self, Box<dyn Error>> {
        if expression.trim().is_empty() {
            return Err("script empty".into());
        }
        let wrapper = format!(
            r#"
                y = (
                    {expression}
                )
            "#
        );

        let vm = Vm::new();
        let program = vm.compile(&wrapper)?;
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let evaluator = Self {
            program,
            _vm: vm,
            x,
            y,
        };
        Ok(evaluator)
    }
}

impl ExpressionEvaluator for Eel2ExpressionEvaluator {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str> {
        let input_value = vars("x", &[]);
        let result = unsafe {
            if let Some(x) = input_value {
                self.x.set(x);
            }
            self.program.execute();
            self.y.get()
        };
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval() {
        // Given
        let expression = "2 * x";
        let evaluator = Eel2ExpressionEvaluator::compile(expression).unwrap();
        // When
        let vars = |name: &str, args: &[f64]| match name {
            "x" => Some(5.0),
            _ => None,
        };
        let result = evaluator.evaluate(vars);
        // Then
        assert_eq!(result, Ok(10.0));
    }

    #[test]
    fn eval_legacy() {
        // Given
        let expression = "y = 2 * x";
        let evaluator = Eel2ExpressionEvaluator::compile(expression).unwrap();
        // When
        let vars = |name: &str, args: &[f64]| match name {
            "x" => Some(5.0),
            _ => None,
        };
        let result = evaluator.evaluate(vars);
        // Then
        assert_eq!(result, Ok(10.0));
    }
}
