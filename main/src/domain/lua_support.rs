use anyhow::anyhow;
use mlua::serde::de;
use mlua::{ChunkMode, Function, Lua, Table, Value, VmState};
use serde::de::DeserializeOwned;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct SafeLua(Lua);

impl SafeLua {
    /// Creates the Lua state.
    pub fn new() -> anyhow::Result<Self> {
        let lua = Lua::new();
        // TODO-medium Maybe we can avoid having to build the safe Lua environment for each
        //  compilation/create-fresh-env step by doing something like the following.
        // // Build safe globals based on original globals
        // let safe_globals = build_safe_lua_env(&lua, lua.globals())?;
        // // Empty original globals
        // let original_keys: Vec<Value> = lua
        //     .globals()
        //     .pairs::<Value, Value>()
        //     .flat_map(|res| res?.0)
        //     .collect();
        // let globals = lua.globals();
        // for key in original_keys {
        //     globals.raw_remove(key)?;
        // }
        // // Fill original globals with safe globals
        // for pair in safe_globals.pairs::<Value, Value>() {
        //     let (key, value) = pair?;
        //     globals[key] = value;
        // }
        Ok(Self(lua))
    }

    pub fn from_value<T>(value: Value) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let result = T::deserialize(de::Deserializer::new(value))?;
        Ok(result)
    }

    /// Compiles as a function with return value (for later execution).
    pub fn compile_as_function(
        &self,
        name: &str,
        code: &str,
        env: Table,
    ) -> anyhow::Result<Function> {
        let chunk = self
            .0
            .load(code)
            .set_name(name)
            .set_environment(env)
            .set_mode(ChunkMode::Text);
        let function = chunk.into_function()?;
        Ok(function)
    }

    /// Compiles and executes the given code in one go (shouldn't be used for repeated execution!).
    pub fn compile_and_execute(
        &self,
        display_name: String,
        code: &str,
        env: Table,
    ) -> anyhow::Result<Value> {
        compile_and_execute(&self.0, display_name, code, env)
    }

    /// Creates a fresh environment for this Lua state.
    ///
    /// Setting `allow_side_effects` unlocks a few more vars, but only use that if you boot up a
    /// fresh Lua state for each execution.
    pub fn create_fresh_environment(&self, allow_side_effects: bool) -> anyhow::Result<Table> {
        create_fresh_environment(&self.0, allow_side_effects)
    }

    /// Call before executing user code in order to prevent code from taking too long to execute.
    pub fn start_execution_time_limit_countdown(&self) {
        const MAX_DURATION: Duration = Duration::from_millis(1000);
        let instant = Instant::now();
        self.0.set_interrupt(move |_lua| {
            if instant.elapsed() > MAX_DURATION {
                Err(mlua::Error::ExternalError(Arc::new(
                    RealearnScriptError::Timeout,
                )))
            } else {
                Ok(VmState::Continue)
            }
        });
    }
}

/// Creates a fresh environment for this Lua state.
///
/// Setting `allow_side_effects` unlocks a few more vars, but only use that if you boot up a
/// fresh Lua state for each execution.
pub fn create_fresh_environment(lua: &Lua, allow_side_effects: bool) -> anyhow::Result<Table> {
    build_safe_lua_env(lua, lua.globals(), allow_side_effects)
}

/// Compiles and executes the given code in one go (shouldn't be used for repeated execution!).
pub fn compile_and_execute(
    lua: &Lua,
    display_name: String,
    code: &str,
    env: Table,
) -> anyhow::Result<Value> {
    let lua_chunk = lua
        .load(code)
        .set_name(display_name)
        .set_mode(ChunkMode::Text)
        .set_environment(env);
    let value = lua_chunk.eval().map_err(|e| match e {
        // Box the cause if it's a callback error (used for the execution time limit feature).
        mlua::Error::CallbackError { cause, .. } => {
            anyhow!(cause)
        }
        e => anyhow!(e),
    })?;
    Ok(value)
}

impl AsRef<Lua> for SafeLua {
    fn as_ref(&self) -> &Lua {
        &self.0
    }
}

#[derive(Debug, derive_more::Display)]
enum RealearnScriptError {
    #[display(fmt = "Helgobox script took too long to execute")]
    Timeout,
}

impl Error for RealearnScriptError {}

/// Creates a Lua environment in which we can't execute potentially malicious code
/// (by only including safe functions according to http://lua-users.org/wiki/SandBoxes).
///
/// Setting `allow_side_effects` unlocks a few more vars, but only use that if you boot up a
/// fresh Lua state for each execution.
fn build_safe_lua_env(
    lua: &Lua,
    original_env: Table,
    allow_side_effects: bool,
) -> anyhow::Result<Table> {
    let safe_env = lua.create_table()?;
    for var in SAFE_LUA_VARS {
        copy_var_to_table(lua, &safe_env, &original_env, var)?;
    }
    if allow_side_effects {
        for var in EXTENDED_SAFE_LUA_VARS {
            copy_var_to_table(lua, &safe_env, &original_env, var)?;
        }
    }
    Ok(safe_env)
}

fn copy_var_to_table(
    lua: &Lua,
    dest_table: &Table,
    src_table: &Table,
    var: &str,
) -> anyhow::Result<()> {
    if let Some(dot_index) = var.find('.') {
        // Nested variable
        let parent_var = &var[0..dot_index];
        let nested_dest_table = if let Ok(t) = dest_table.get(parent_var) {
            t
        } else {
            let new_table = lua.create_table()?;
            dest_table.set(parent_var, new_table.clone())?;
            new_table
        };
        let nested_src_table: Table = src_table.get(parent_var)?;
        let child_var = &var[dot_index + 1..];
        copy_var_to_table(lua, &nested_dest_table, &nested_src_table, child_var)?;
        Ok(())
    } else {
        // Leaf variable
        let original_value: Value = src_table.get(var)?;
        dest_table.set(var, original_value)?;
        Ok(())
    }
}

/// Safe Lua vars according to http://lua-users.org/wiki/SandBoxes.
///
/// Even a bit more restrictive because we don't include `io` and `coroutine`.
const SAFE_LUA_VARS: &[&str] = &[
    "assert",
    "error",
    "ipairs",
    "next",
    "pairs",
    "pcall",
    "print",
    "select",
    "tonumber",
    "tostring",
    "type",
    "unpack",
    "_VERSION",
    "xpcall",
    "string.byte",
    "string.char",
    "string.find",
    "string.format",
    "string.gmatch",
    "string.gsub",
    "string.len",
    "string.lower",
    "string.match",
    "string.rep",
    "string.reverse",
    "string.sub",
    "string.upper",
    // "table.clone" is available in Luau only
    "table.clone",
    "table.insert",
    "table.maxn",
    "table.remove",
    "table.sort",
    "math.abs",
    "math.acos",
    "math.asin",
    "math.atan",
    "math.atan2",
    "math.ceil",
    "math.cos",
    "math.cosh",
    "math.deg",
    "math.exp",
    "math.floor",
    "math.fmod",
    "math.frexp",
    "math.huge",
    "math.ldexp",
    "math.log",
    "math.log10",
    "math.max",
    "math.min",
    "math.modf",
    "math.pi",
    "math.pow",
    "math.rad",
    "math.random",
    "math.sin",
    "math.sinh",
    "math.sqrt",
    "math.tan",
    "math.tanh",
    "os.clock",
    "os.difftime",
    "os.time",
    // bit32 (Lua 5.2 & Luau intersection)
    "bit32.arshift",
    "bit32.band",
    "bit32.bnot",
    "bit32.bor",
    "bit32.btest",
    "bit32.bxor",
    "bit32.extract",
    "bit32.lrotate",
    "bit32.lshift",
    "bit32.replace",
    "bit32.rrotate",
    "bit32.rshift",
];

/// An extended set of Lua vars that can be considered safe under certain circumstances.
///
/// Some vars are unsafe according to http://lua-users.org/wiki/SandBoxes, but only because of the
/// side effects that it could have on other code executed in the same Lua state. In situations
/// where we create a fresh Lua state everytime, this doesn't matter.
const EXTENDED_SAFE_LUA_VARS: &[&str] = &["setmetatable"];
