use crate::base::SendOrSyncWhatever;
use crate::domain::{SafeLua, ScriptColor, ScriptFeedbackEvent, ScriptFeedbackValue};
use helgoboss_learn::{
    FeedbackScript, FeedbackScriptInput, FeedbackScriptOutput, FeedbackValue, NumericValue,
    PropProvider, PropValue,
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
            let prop = lua.create_function(move |_, key: String| {
                let prop_provider: &dyn PropProvider = unsafe { trafficker.get() };
                let prop_value = prop_provider.get_prop_value(&key);
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

/// This utility provides a way to pass a trait object reference that is neither `Send` nor
/// `'static` into functions that require these traits.
///
/// Dangerous stuff and rarely necessary! You go down to C level with this.
struct Trafficker {
    thin_ptr: *const c_void,
}

unsafe impl Send for Trafficker {}

impl Trafficker {
    /// Put a reference to a trait object reference in here (`&&dyn ...`).
    ///
    /// We need a reference to a reference here because
    pub fn new<T: Copy>(thin_ref: &T) -> Self {
        let thin_ptr = thin_ref as *const _ as *const c_void;
        Self { thin_ptr }
    }

    /// Get it out again.
    ///
    /// Make sure you use the same type as in `new`! We can't make `T` a type parameter of the
    /// struct because otherwise the borrow checker would complain that things go out of scope.
    ///
    /// # Safety
    ///
    /// If you don't provide the proper type or the reference passed to `new` went out of scope,
    /// things crash horribly.
    pub unsafe fn get<T: Copy>(&self) -> T {
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
