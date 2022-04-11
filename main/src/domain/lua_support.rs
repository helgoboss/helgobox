use mlua::{HookTriggers, Lua, Table, Value};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct SafeLua(Lua);

impl SafeLua {
    /// Creates the Lua state.
    pub fn new() -> Result<Self, Box<dyn Error>> {
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

    /// Creates a fresh environment for this Lua state.
    pub fn create_fresh_environment(&self) -> Result<Table, Box<dyn Error>> {
        build_safe_lua_env(&self.0, self.0.globals())
    }

    /// Call before executing user code in order to prevent code from taking too long to execute.
    pub fn start_execution_time_limit_countdown(
        self,
        max_duration: Duration,
    ) -> Result<Self, Box<dyn Error>> {
        let instant = Instant::now();
        self.0.set_hook(
            HookTriggers::every_nth_instruction(10),
            move |_lua, _debug| {
                if instant.elapsed() > max_duration {
                    Err(mlua::Error::ExternalError(Arc::new(
                        RealearnScriptError::Timeout,
                    )))
                } else {
                    Ok(())
                }
            },
        )?;
        Ok(self)
    }
}

impl AsRef<Lua> for SafeLua {
    fn as_ref(&self) -> &Lua {
        &self.0
    }
}

#[derive(Debug, derive_more::Display)]
enum RealearnScriptError {
    #[display(fmt = "ReaLearn script took too long to execute")]
    Timeout,
}

impl Error for RealearnScriptError {}

/// Creates a Lua environment in which we can't execute potentially malicious code
/// (by only including safe functions according to http://lua-users.org/wiki/SandBoxes).
fn build_safe_lua_env<'a>(lua: &'a Lua, original_env: Table) -> Result<Table<'a>, Box<dyn Error>> {
    let safe_env = lua.create_table()?;
    for var in SAFE_LUA_VARS {
        copy_var_to_table(lua, &safe_env, &original_env, var)?;
    }
    Ok(safe_env)
}

fn copy_var_to_table(
    lua: &Lua,
    dest_table: &Table,
    src_table: &Table,
    var: &str,
) -> Result<(), Box<dyn Error>> {
    if let Some(dot_index) = var.find('.') {
        // Nested variable
        let parent_var = &var[0..dot_index];
        let nested_dest_table = if let Ok(t) = dest_table.get::<_, Table>(parent_var) {
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
];
