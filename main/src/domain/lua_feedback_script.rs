use crate::domain::{SafeLua, ScriptColor, ScriptFeedbackEvent};
use base::Trafficker;
use helgoboss_learn::{
    FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput, FeedbackValue, NumericValue,
    PropProvider, PropValue,
};
use mlua::{Function, Lua, LuaSerdeExt, Table, ToLua, Value};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error;

#[derive(Debug)]
pub struct LuaFeedbackScript<'lua> {
    lua: &'lua SafeLua,
    function: Function<'lua>,
    env: Table<'lua>,
    context_key: Value<'lua>,
}

unsafe impl<'a> Send for LuaFeedbackScript<'a> {}

impl<'lua> LuaFeedbackScript<'lua> {
    pub fn compile(lua: &'lua SafeLua, lua_script: &str) -> Result<Self, Box<dyn Error>> {
        if lua_script.trim().is_empty() {
            return Err("script empty".into());
        }
        let env = lua.create_fresh_environment(false)?;
        let function = lua.compile_as_function("Feedback script", lua_script, env.clone())?;
        let script = Self {
            lua,
            env,
            function,
            context_key: "context".to_lua(lua.as_ref())?,
        };
        Ok(script)
    }

    fn feedback_internal(
        &self,
        input: FeedbackScriptInput,
    ) -> Result<FeedbackScriptOutput, Box<dyn Error>> {
        let lua = self.lua.as_ref();
        let thin_ref = &input.prop_provider;
        // We need to use the Trafficker here because mlua requires the input to create_function()
        // to be 'static and Send. However, here we have a Rust function that doesn't fulfill any
        // of these requirements, so create_function() would complain. However, in this case, the
        // requirements are unnecessarily strict. Because in our usage scenario (= synchronous
        // immediate execution, just once), the function can't go out of scope and we also don't
        // send anything to another thread.
        let trafficker = Trafficker::new(thin_ref);
        // Build input data
        let context_table = {
            let table = lua.create_table()?;
            table.set("mode", 0)?;
            let prop = lua.create_function(move |_, key: String| {
                let prop_provider: &dyn PropProvider = unsafe { trafficker.get() };
                let prop_value = prop_provider.get_prop_value(&key);
                Ok(prop_value.map(LuaPropValue))
            })?;
            table.set("prop", prop)?;
            table
        };
        self.env.raw_set(self.context_key.clone(), context_table)?;
        // Invoke script
        let value: Value = self.function.call(())?;
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

struct LuaPropValue(PropValue);

impl<'lua> ToLua<'lua> for LuaPropValue {
    fn to_lua(self, lua: &'lua Lua) -> mlua::Result<Value<'lua>> {
        match self.0 {
            PropValue::Normalized(p) => p.get().to_lua(lua),
            PropValue::Index(i) => i.to_lua(lua),
            PropValue::Numeric(NumericValue::Decimal(i)) => i.to_lua(lua),
            PropValue::Numeric(NumericValue::Discrete(i)) => i.to_lua(lua),
            PropValue::Boolean(state) => state.to_lua(lua),
            PropValue::Text(t) => t.to_lua(lua),
            PropValue::Color(c) => {
                let script_color = ScriptColor::from(c);
                lua.to_value(&script_color)
            }
            PropValue::DurationInMillis(d) => d.to_lua(lua),
        }
    }
}

impl<'a> FeedbackScript for LuaFeedbackScript<'a> {
    fn feedback(
        &self,
        input: FeedbackScriptInput,
    ) -> Result<FeedbackScriptOutput, Cow<'static, str>> {
        self.feedback_internal(input)
            .map_err(|e| e.to_string().into())
    }

    fn used_props(&self) -> Result<HashSet<String>, Box<dyn Error>> {
        let prop_provider = TrackingPropProvider::default();
        let input = FeedbackScriptInput {
            prop_provider: &prop_provider,
        };
        self.feedback_internal(input)?;
        Ok(prop_provider.used_props.take())
    }
}

#[derive(Default)]
struct TrackingPropProvider {
    used_props: RefCell<HashSet<String>>,
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
        assert_eq!(
            used_props,
            HashSet::from(["hello".to_string(), "bye".to_string()])
        );
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
        let output = script.feedback(input).unwrap();
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
        let output = script.feedback(input).unwrap();
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
        let output = script.feedback(input).unwrap();
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
        let output = script.feedback(input).unwrap();
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
