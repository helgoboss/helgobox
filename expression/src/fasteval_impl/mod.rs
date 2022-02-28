use crate::api::ExpressionEvaluator;
use fasteval::{Compiler, Evaler, Instruction, Slab};
use std::error::Error;

pub struct FastevalExpressionEvaluator {
    slab: Slab,
    instruction: Instruction,
}

impl FastevalExpressionEvaluator {
    pub fn compile(expression: &str) -> Result<Self, Box<dyn Error>> {
        let parser = fasteval::Parser::new();
        let mut slab = fasteval::Slab::new();
        let instruction = parser
            .parse(expression, &mut slab.ps)?
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);
        let evaluator = Self { slab, instruction };
        Ok(evaluator)
    }

    fn evaluate_internal(
        &self,
        vars: impl Fn(&str, &[f64]) -> Option<f64>,
    ) -> Result<f64, fasteval::Error> {
        use fasteval::eval_compiled_ref;
        let mut cb = |name: &str, args: Vec<f64>| -> Option<f64> {
            // Use-case specific variables
            vars(name, &args)
        };
        let res = eval_compiled_ref!(&self.instruction, &self.slab, &mut cb);
        Ok(res)
    }
}

impl ExpressionEvaluator for FastevalExpressionEvaluator {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str> {
        self.evaluate_internal(vars)
            .map_err(|_| "couldn't evaluate expression")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval() {
        // Given
        let expression = "2 * x";
        let evaluator = FastevalExpressionEvaluator::compile(expression).unwrap();
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
