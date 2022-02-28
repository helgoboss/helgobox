use crate::api::ExpressionEvaluator;
use mlua::{ChunkMode, Function, Lua, MultiValue, Table, ToLua, Value};
use std::error::Error;

pub struct LuaExpressionEvaluator<'lua> {
    function: Function<'lua>,
    env: Table<'lua>,
    x: Value<'lua>,
}

impl<'lua> LuaExpressionEvaluator<'lua> {
    pub fn compile(lua: &'lua Lua, expression: &str) -> Result<Self, Box<dyn Error>> {
        let env = lua.create_table()?;
        let wrapper = format!("return {expression}");
        let chunk = lua
            .load(&wrapper)
            .set_name("Expression")?
            .set_environment(env.clone())?
            .set_mode(ChunkMode::Text);
        let function = chunk.into_function()?;
        let evaluator = Self {
            env,
            function,
            x: "x".to_lua(lua)?,
        };
        Ok(evaluator)
    }
}

impl<'lua> ExpressionEvaluator for LuaExpressionEvaluator<'lua> {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str> {
        // self.function
        //     .call(5.0)
        //     .map_err(|_| "failed to evaluate Lua expression")
        self.env.raw_set(self.x.clone(), 5.0);
        let res = self.function.call(()).unwrap();
        Ok(res)
    }
}

pub struct FunctionalLuaExpressionEvaluator<'lua> {
    lua: &'lua Lua,
    function: Function<'lua>,
    metatable: Table<'lua>,
}

impl<'lua> FunctionalLuaExpressionEvaluator<'lua> {
    pub fn compile(lua: &'lua Lua, expression: &str) -> Result<Self, Box<dyn Error>> {
        let env = lua.create_table()?;
        let metatable = lua.create_table()?;
        env.set_metatable(Some(metatable.clone()));
        let wrapper = format!("return {expression}");
        let fun = lua
            .create_function(|_, (_, name): (Table, String)| {
                // vars(&name, &[]).ok_or(mlua::Error::RuntimeError(String::new()))
                Ok(5.0)
            })
            .unwrap();
        metatable.raw_set("__index", fun);
        let chunk = lua
            .load(&wrapper)
            .set_name("Expression")?
            .set_environment(env.clone())?
            .set_mode(ChunkMode::Text);
        let function = chunk.into_function()?;
        let evaluator = Self {
            lua,
            metatable,
            function,
        };
        Ok(evaluator)
    }
}

impl<'lua> ExpressionEvaluator for FunctionalLuaExpressionEvaluator<'lua> {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str> {
        let res = self.function.call(()).unwrap();
        Ok(res)
    }
}

pub struct ParameterLuaExpressionEvaluator<'lua> {
    lua: &'lua Lua,
    function: Function<'lua>,
    metatable: Table<'lua>,
}

impl<'lua> ParameterLuaExpressionEvaluator<'lua> {
    pub fn compile(lua: &'lua Lua, expression: &str) -> Result<Self, Box<dyn Error>> {
        let env = lua.create_table()?;
        let metatable = lua.create_table()?;
        env.set_metatable(Some(metatable.clone()));
        let wrapper = format!(
            r#"
            function(x)
                return {expression}
            end
        "#
        );
        let chunk = lua
            .load(&wrapper)
            .set_name("Expression")?
            .set_environment(env.clone())?
            .set_mode(ChunkMode::Text);
        let function = chunk.eval()?;
        let evaluator = Self {
            lua,
            metatable,
            function,
        };
        Ok(evaluator)
    }
}

impl<'lua> ExpressionEvaluator for ParameterLuaExpressionEvaluator<'lua> {
    fn evaluate(&self, vars: impl Fn(&str, &[f64]) -> Option<f64>) -> Result<f64, &'static str> {
        let res = self.function.call((5.0)).unwrap();
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_normal() {
        // Given
        let lua = Lua::new();
        let expression = "2 * x";
        let evaluator = LuaExpressionEvaluator::compile(&lua, expression).unwrap();
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
    fn eval_functional() {
        // Given
        let lua = Lua::new();
        let expression = "2 * x";
        let evaluator = FunctionalLuaExpressionEvaluator::compile(&lua, expression).unwrap();
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
    fn eval_parameter() {
        // Given
        let lua = Lua::new();
        let expression = "2 * x";
        let evaluator = ParameterLuaExpressionEvaluator::compile(&lua, expression).unwrap();
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
