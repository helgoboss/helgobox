use crate::domain::{lua_module_path_without_ext, SafeLua, ScriptColor, ScriptFeedbackEvent};
use anyhow::ensure;
use base::hash_util::NonCryptoHashSet;
use helgoboss_learn::{
    FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput, FeedbackValue, NumericValue,
    PropProvider, PropValue,
};
use mlua::{Function, IntoLua, Lua, LuaSerdeExt, Table, Value};
use std::borrow::Cow;
use std::cell::RefCell;
use std::error::Error;

#[derive(Copy, Clone, Debug, Default)]
pub struct AdditionalLuaFeedbackScriptInput<'a> {
    pub compartment_lua: Option<&'a mlua::Value>,
}

#[derive(Debug)]
pub struct LuaFeedbackScript<'a> {
    lua: &'a SafeLua,
    function: Function,
    env: Table,
    context_key: Value,
}

unsafe impl Send for LuaFeedbackScript<'_> {}

impl<'a> LuaFeedbackScript<'a> {
    pub fn compile(lua: &'a SafeLua, lua_script: &str) -> anyhow::Result<Self> {
        ensure!(!lua_script.trim().is_empty(), "script empty");
        let env = lua.create_fresh_environment(false)?;
        let function = lua.compile_as_function("Feedback script", lua_script, env.clone())?;
        let script = Self {
            lua,
            env,
            function,
            context_key: "context".into_lua(lua.as_ref())?,
        };
        Ok(script)
    }

    fn feedback_internal(
        &self,
        input: FeedbackScriptInput,
        additional_input: <LuaFeedbackScript<'a> as FeedbackScript<'a>>::AdditionalInput,
    ) -> anyhow::Result<FeedbackScriptOutput> {
        let lua = self.lua.as_ref();
        let value = lua.scope(|scope| {
            // Set require function
            let require = scope.create_function(move |lua, path: String| {
                let val = match lua_module_path_without_ext(&path) {
                    LUA_FEEDBACK_SCRIPT_RUNTIME_NAME => create_lua_feedback_script_runtime(lua),
                    "compartment" => {
                        additional_input.compartment_lua.cloned().unwrap_or(Value::Nil)
                    },
                    _ => return Err(mlua::Error::runtime(format!("Feedback scripts don't support the usage of 'require' for anything else than '{LUA_FEEDBACK_SCRIPT_RUNTIME_NAME}' and 'compartment'!")))
                };
                Ok(val)
            })
                .map_err(mlua::Error::runtime)?;
            self.env.raw_set("require", require)
                .map_err(mlua::Error::runtime)?;
            // Build input data
            let context_table = {
                let table = lua.create_table()?;
                table.set("mode", 0)?;
                let prop = scope.create_function(move |_, key: String| {
                    let prop_value = input.prop_provider.get_prop_value(&key);
                    Ok(prop_value.map(LuaPropValue))
                })?;
                table.set("prop", prop)?;
                table
            };
            self.env.raw_set(self.context_key.clone(), context_table)?;
            // Invoke script
            let value: Value = self.function.call(())?;
            Ok(value)
        })?;
        // Process return value
        let output: LuaScriptFeedbackOutput = self.lua.as_ref().from_value(value)?;
        let feedback_value = match output.feedback_event {
            None => FeedbackValue::Off,
            Some(e) => e.into_api_feedback_value(),
        };
        let api_output = FeedbackScriptOutput { feedback_value };
        Ok(api_output)
    }
}

pub const LUA_FEEDBACK_SCRIPT_RUNTIME_NAME: &str = "feedback_script_runtime";

pub fn create_lua_feedback_script_runtime(_lua: &Lua) -> mlua::Value {
    // At the moment, the feedback script runtime doesn't contain any functions, just types.
    // That means it's only relevant for autocompletion in the IDE. We can return nil.
    Value::Nil
}

struct LuaPropValue(PropValue);

impl IntoLua for LuaPropValue {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        match self.0 {
            PropValue::Normalized(p) => p.get().into_lua(lua),
            PropValue::Index(i) => i.into_lua(lua),
            PropValue::Numeric(NumericValue::Decimal(i)) => i.into_lua(lua),
            PropValue::Numeric(NumericValue::Discrete(i)) => i.into_lua(lua),
            PropValue::Boolean(state) => state.into_lua(lua),
            PropValue::Text(t) => t.into_lua(lua),
            PropValue::Color(c) => {
                let script_color = ScriptColor::from(c);
                lua.to_value(&script_color)
            }
            PropValue::DurationInMillis(d) => d.into_lua(lua),
        }
    }
}

impl<'a> FeedbackScript<'a> for LuaFeedbackScript<'a> {
    type AdditionalInput = AdditionalLuaFeedbackScriptInput<'a>;

    fn feedback(
        &self,
        input: FeedbackScriptInput,
        additional_input: Self::AdditionalInput,
    ) -> Result<FeedbackScriptOutput, Cow<'static, str>> {
        self.feedback_internal(input, additional_input)
            .map_err(|e| e.to_string().into())
    }

    fn used_props(&self) -> Result<NonCryptoHashSet<String>, Box<dyn Error>> {
        let prop_provider = TrackingPropProvider::default();
        let input = FeedbackScriptInput {
            prop_provider: &prop_provider,
        };
        self.feedback_internal(input, Default::default())?;
        Ok(prop_provider.used_props.take())
    }
}

#[derive(Default)]
struct TrackingPropProvider {
    used_props: RefCell<NonCryptoHashSet<String>>,
}

impl PropProvider for TrackingPropProvider {
    fn get_prop_value(&self, key: &str) -> Option<PropValue> {
        self.used_props.borrow_mut().insert(key.to_string());
        None
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct LuaScriptFeedbackOutput {
    feedback_event: Option<ScriptFeedbackEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_learn::{
        AbsoluteValue, FeedbackStyle, NumericFeedbackValue, PropValue, RgbColor,
        TextualFeedbackValue, UnitValue,
    };

    #[test]
    fn used_props() {
        // Given
        let text = r#"
            local foo = context.prop("bye")
            local bla = context.prop("hello")
            return {
                feedback_event = nil
            }
        "#;
        let lua = SafeLua::new().unwrap();
        let script = LuaFeedbackScript::compile(&lua, text).unwrap();
        // When
        let used_props = script.used_props().unwrap();
        // Then
        let expected: NonCryptoHashSet<_> = ["hello".to_string(), "bye".to_string()]
            .into_iter()
            .collect();
        assert_eq!(used_props, expected);
    }

    #[test]
    fn off_feedback() {
        // Given
        let text = r#"
            return {
                feedback_event = nil
            }
        "#;
        let lua = SafeLua::new().unwrap();
        let script = LuaFeedbackScript::compile(&lua, text).unwrap();
        // When
        let input = FeedbackScriptInput {
            prop_provider: &|_: &str| None,
        };
        let output = script.feedback(input, Default::default()).unwrap();
        // Then
        assert_eq!(output.feedback_value, FeedbackValue::Off);
    }

    #[test]
    fn numeric_feedback() {
        // Given
        let text = r#"
            return {
                feedback_event = {
                    value = 5,
                    color = { r = 23, g = 5, b = 122 },
                },
            }
        "#;
        let lua = SafeLua::new().unwrap();
        let script = LuaFeedbackScript::compile(&lua, text).unwrap();
        // When
        let input = FeedbackScriptInput {
            prop_provider: &|_: &str| None,
        };
        let output = script.feedback(input, Default::default()).unwrap();
        // Then
        assert_eq!(
            output.feedback_value,
            FeedbackValue::Numeric(NumericFeedbackValue::new(
                FeedbackStyle {
                    color: Some(RgbColor::new(23, 5, 122)),
                    background_color: None,
                },
                AbsoluteValue::Continuous(UnitValue::MAX)
            ))
        );
    }

    #[test]
    fn text_feedback() {
        // Given
        let text = r#"
            return {
                feedback_event = {
                    value = "hello"
                },
            }
        "#;
        let lua = SafeLua::new().unwrap();
        let script = LuaFeedbackScript::compile(&lua, text).unwrap();
        // When
        let input = FeedbackScriptInput {
            prop_provider: &|_: &str| None,
        };
        let output = script.feedback(input, Default::default()).unwrap();
        // Then
        assert_eq!(
            output.feedback_value,
            FeedbackValue::Textual(TextualFeedbackValue::new(
                FeedbackStyle::default(),
                "hello".into()
            ))
        );
    }

    #[test]
    fn text_feedback_with_props() {
        // Given
        let text = r#"
            return {
                feedback_event = {
                    value = context.prop("name")
                },
            }
        "#;
        let lua = SafeLua::new().unwrap();
        let script = LuaFeedbackScript::compile(&lua, text).unwrap();
        // When
        let input = FeedbackScriptInput {
            prop_provider: &|key: &str| match key {
                "name" => Some(PropValue::Text("hello".into())),
                _ => None,
            },
        };
        let output = script.feedback(input, Default::default()).unwrap();
        // Then
        assert_eq!(
            output.feedback_value,
            FeedbackValue::Textual(TextualFeedbackValue::new(
                FeedbackStyle::default(),
                "hello".into()
            ))
        );
    }
}
