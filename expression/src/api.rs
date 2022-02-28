pub trait ExpressionEvaluator {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str>;
}
