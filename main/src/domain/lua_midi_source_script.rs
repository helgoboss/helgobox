use crate::domain::SafeLua;
use helgoboss_learn::{
    AbsoluteValue, FeedbackValue, MidiSourceAddress, MidiSourceScript, MidiSourceScriptOutcome,
    RawMidiEvent, RgbColor,
};
use mlua::{Function, LuaSerdeExt, Table, ToLua, Value};
use std::error::Error;

#[derive(Clone, Debug)]
pub struct LuaMidiSourceScript<'lua> {
    lua: &'lua SafeLua,
    function: Function<'lua>,
    env: Table<'lua>,
    y_key: Value<'lua>,
    context_key: Value<'lua>,
}

unsafe impl<'a> Send for LuaMidiSourceScript<'a> {}

impl<'lua> LuaMidiSourceScript<'lua> {
    pub fn compile(lua: &'lua SafeLua, lua_script: &str) -> Result<Self, Box<dyn Error>> {
        if lua_script.trim().is_empty() {
            return Err("script empty".into());
        }
        let env = lua.create_fresh_environment(false)?;
        let function = lua.compile_as_function("MIDI source script", lua_script, env.clone())?;
        let script = Self {
            lua,
            env,
            function,
            y_key: "y".to_lua(lua.as_ref())?,
            context_key: "context".to_lua(lua.as_ref())?,
        };
        Ok(script)
    }
}

#[derive(serde::Serialize)]
struct ScriptContext {
    feedback_event: ScriptFeedbackEvent,
}

#[derive(serde::Serialize)]
struct ScriptFeedbackEvent {
    color: Option<ScriptColor>,
    background_color: Option<ScriptColor>,
}

#[derive(serde::Serialize)]
struct ScriptColor {
    r: u8,
    g: u8,
    b: u8,
}

impl From<RgbColor> for ScriptColor {
    fn from(c: RgbColor) -> Self {
        Self {
            r: c.r(),
            g: c.g(),
            b: c.b(),
        }
    }
}

impl<'a> MidiSourceScript for LuaMidiSourceScript<'a> {
    fn execute(&self, input_value: FeedbackValue) -> Result<MidiSourceScriptOutcome, &'static str> {
        // TODO-medium We don't limit the time of each execution at the moment because not sure
        //  how expensive this measurement is. But it would actually be useful to do it for MIDI
        //  scripts!
        // Build input data
        let context = ScriptContext {
            feedback_event: ScriptFeedbackEvent {
                color: input_value.color().map(ScriptColor::from),
                background_color: input_value.background_color().map(ScriptColor::from),
            },
        };
        let y_value = match input_value {
            FeedbackValue::Off => Value::Nil,
            FeedbackValue::Numeric(n) => match n.value {
                AbsoluteValue::Continuous(v) => Value::Number(v.get()),
                AbsoluteValue::Discrete(f) => Value::Integer(f.actual() as _),
            },
            FeedbackValue::Textual(v) => v
                .text
                .to_lua(self.lua.as_ref())
                .map_err(|_| "couldn't convert string to Lua string")?,
        };
        // Set input data as variables "y" and "context".
        self.env
            .raw_set(self.y_key.clone(), y_value)
            .map_err(|_| "couldn't set y variable")?;
        let context_lua_value = self.lua.as_ref().to_value(&context).unwrap();
        self.env
            .raw_set(self.context_key.clone(), context_lua_value)
            .map_err(|_| "couldn't set context variable")?;
        // Invoke script
        let value: Value = self
            .function
            .call(())
            .map_err(|_| "failed to invoke Lua script")?;
        // Process return value
        let outcome: LuaScriptOutcome = self
            .lua
            .as_ref()
            .from_value(value)
            .map_err(|_| "Lua script result has wrong type")?;
        let events = outcome
            .messages
            .into_iter()
            .flat_map(|msg| RawMidiEvent::try_from_slice(0, &msg))
            .collect();
        let outcome = MidiSourceScriptOutcome {
            address: outcome
                .address
                .map(|bytes| MidiSourceAddress::Script { bytes }),
            events,
        };
        Ok(outcome)
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct LuaScriptOutcome {
    address: Option<u64>,
    messages: Vec<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use helgoboss_learn::{
        FeedbackStyle, NumericFeedbackValue, RgbColor, TextualFeedbackValue, UnitValue,
    };

    #[test]
    fn basics() {
        // Given
        let text = "
            return {
                address = 0x4bb0,
                messages = {
                    { 0xb0, 0x4b, math.floor(y * 10) }
                }
            }
        ";
        let lua = SafeLua::new().unwrap();
        let script = LuaMidiSourceScript::compile(&lua, text).unwrap();
        // When
        let fb_value = NumericFeedbackValue::new(
            FeedbackStyle::default(),
            AbsoluteValue::Continuous(UnitValue::new(0.5)),
        );
        let outcome = script.execute(FeedbackValue::Numeric(fb_value)).unwrap();
        // Then
        assert_eq!(
            outcome.address,
            Some(MidiSourceAddress::Script { bytes: 0x4bb0 })
        );
        assert_eq!(
            outcome.events,
            vec![RawMidiEvent::try_from_slice(0, &[0xb0, 0x4b, 5]).unwrap()]
        );
    }

    #[test]
    fn text_feedback_value() {
        // Given
        let text = "
            local lookup_table = {
                playing = 5,
                stopped = 6,
                paused = 7,
            }
            return {
                messages = {
                    { 0xb0, 0x4b, lookup_table[y] or 0 }
                }
            }
        ";
        let lua = SafeLua::new().unwrap();
        let script = LuaMidiSourceScript::compile(&lua, text).unwrap();
        // When
        let matched_outcome = script
            .execute(FeedbackValue::Textual(TextualFeedbackValue::new(
                FeedbackStyle::default(),
                "playing".into(),
            )))
            .unwrap();
        let unmatched_outcome = script
            .execute(FeedbackValue::Numeric(NumericFeedbackValue::new(
                FeedbackStyle::default(),
                AbsoluteValue::Continuous(UnitValue::MAX),
            )))
            .unwrap();
        // Then
        assert_eq!(matched_outcome.address, None);
        assert_eq!(
            matched_outcome.events,
            vec![RawMidiEvent::try_from_slice(0, &[0xb0, 0x4b, 5]).unwrap()]
        );
        assert_eq!(unmatched_outcome.address, None);
        assert_eq!(
            unmatched_outcome.events,
            vec![RawMidiEvent::try_from_slice(0, &[0xb0, 0x4b, 0]).unwrap()]
        );
    }

    #[test]
    fn colors() {
        // Given
        let text = "
            local color = context.feedback_event.color
            if color == nil then
                -- This means no specific color is set. Choose whatever you need.
                color = { r = 0, g = 0, b = 0 }
            end
            return {
                -- A unique number that identifies the LED/display.
                -- (Necessary if you want correct lights-off behavior and coordination
                -- between multiple mappings using the same LED/display).
                address = 0x4b,
                -- Whatever messages your device needs to set that color.
                messages = {
                    { 0xf0, 0x02, 0x4b, color.r, color.g, color.b, 0xf7 }
                }
            }
        ";
        let lua = SafeLua::new().unwrap();
        let script = LuaMidiSourceScript::compile(&lua, text).unwrap();
        // When
        let style = FeedbackStyle {
            color: Some(RgbColor::new(255, 0, 255)),
            background_color: None,
        };
        let value = NumericFeedbackValue::new(style, Default::default());
        let outcome = script.execute(FeedbackValue::Numeric(value)).unwrap();
        // Then
        assert_eq!(
            outcome.address,
            Some(MidiSourceAddress::Script { bytes: 0x4b })
        );
        assert_eq!(
            outcome.events,
            vec![
                RawMidiEvent::try_from_slice(0, &[0xf0, 0x02, 0x4b, 0xff, 0x00, 0xff, 0xf7])
                    .unwrap()
            ]
        );
    }
}
