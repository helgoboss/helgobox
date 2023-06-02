use crate::base::SendOrSyncWhatever;
use crate::domain::{SafeLua, ScriptColor, ScriptFeedbackEvent, ScriptFeedbackValue};
use helgoboss_learn::{
    FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput, FeedbackValue, NumericValue,
    PropValue,
};
use mlua::{Function, Lua, LuaSerdeExt, Table, ToLua, Value};
use std::borrow::Cow;
use std::error::Error;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem;

#[derive(Clone, Debug)]
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
        // We use the mlua "send" feature to make Lua instances implement Send. However, here
        // for once we have a Rust function that is not Send, so create_function() complains.
        // We ignore this because in our usage scenario (= async immediate execution), we don't
        // send anything to another thread.
        let thin_ref = &input.get_prop_value;
        let thin_ptr = thin_ref as *const _ as *const c_void;
        let thin_ptr_wrapper = unsafe { SendOrSyncWhatever::new(thin_ptr) };
        // Build input data
        let context_table = {
            let table = lua.create_table()?;
            let prop = lua.create_function(move |_, key: String| {
                let thin_ptr = *thin_ptr_wrapper.get();
                let thin_ref = unsafe { &*(thin_ptr as *const &dyn Fn(&str) -> Option<PropValue>) };
                let get_prop_value = *thin_ref;
                let prop_value = (get_prop_value)(&key);
                Ok(prop_value.map(LuaPropValue))
            })?;
            table.set("prop", prop)?;
            table
        };
        self.env.raw_set("context", context_table)?;
        // Invoke script
        let value: Value = self.function.call(())?;
        // Process return value
        let output: LuaScriptOutput = self.lua.as_ref().from_value(value)?;
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
            PropValue::Text(t) => t.to_lua(lua),
            PropValue::Color(c) => {
                let script_color = ScriptColor::from(c);
                lua.to_value(&script_color)
            }
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

    fn used_props(&self) -> Vec<String> {
        todo!()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct LuaScriptOutput {
    feedback_event: Option<ScriptFeedbackEvent>,
}

struct Trafficker<T> {
    thin_ptr: *const c_void,
    _p: PhantomData<T>,
}

unsafe impl<T> Send for Trafficker<T> {}

impl<T: Copy> Trafficker<T> {
    pub fn new(val: T) -> Self {
        let thin_ref = &val;
        let thin_ptr = thin_ref as *const _ as *const c_void;
        Self {
            thin_ptr,
            _p: Default::default(),
        }
    }

    pub unsafe fn get_ref(&self) -> T {
        *(self.thin_ptr as *const T)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_learn::{
        AbsoluteValue, FeedbackStyle, NumericFeedbackValue, PropValue, RgbColor,
        TextualFeedbackValue, UnitValue,
    };

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
            get_prop_value: &|_| None,
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
            get_prop_value: &|_| None,
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
            get_prop_value: &|_| None,
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
            get_prop_value: &|key| match key {
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
