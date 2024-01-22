use crate::domain::{compile_and_execute, create_fresh_environment};
use anyhow::Context;
use camino::Utf8Path;
use include_dir::Dir;
use mlua::{Function, Lua, Table, Value};
use std::borrow::Cow;

pub struct LuaModuleContainer<F> {
    finder: F,
    // modules: HashMap<String, Value<'a>>,
}

pub trait LuaModuleFinder {
    fn find_source_by_path(&self, path: &str) -> Option<Cow<'static, str>>;
}

impl<F> LuaModuleContainer<F>
where
    F: LuaModuleFinder + Clone + 'static,
{
    pub fn new(finder: F) -> Self {
        Self {
            finder,
            // modules: Default::default(),
        }
    }

    pub fn execute_as_module<'lua>(
        &self,
        lua: &'lua Lua,
        name: &str,
        code: &str,
    ) -> anyhow::Result<Value<'lua>> {
        execute_as_module(name, code, self.finder.clone(), lua)
    }
}

fn find_and_execute_module<'lua, 'b>(
    finder: impl LuaModuleFinder + Clone + 'static,
    lua: &'lua Lua,
    path: &'b str,
) -> anyhow::Result<Value<'lua>> {
    let path = Utf8Path::new(path);
    let source = if path.extension().is_some() {
        // Extension given. Just get file directly.
        finder
            .find_source_by_path(path.as_str())
            .with_context(|| format!("Couldn't find Lua module [{path}]"))?
    } else {
        // No extension given. Try ".luau" and ".lua".
        ["luau", "lua"]
            .into_iter()
            .find_map(|ext| finder.find_source_by_path(path.with_extension(ext).as_str()))
            .with_context(|| {
                format!(
                    "Couldn't find Lua module [{path}]. Tried both with .lua and .luau extension."
                )
            })?
    };
    execute_as_module(path.as_str(), source.as_ref(), finder, lua)
}

fn execute_as_module<'a, 'b, 'lua>(
    name: &'a str,
    code: &'b str,
    finder: impl LuaModuleFinder + Clone + 'static,
    lua: &'lua Lua,
) -> anyhow::Result<Value<'lua>> {
    let env = create_fresh_environment(lua, true)?;
    let require = create_require_function(finder, lua)?;
    env.set("require", require)?;
    let value = compile_and_execute(lua, name, code, env)
        .with_context(|| format!("Couldn't compile and execute Lua module {name}"))?;
    Ok(value)
    // TODO-high CONTINUE It doesn't seem to be straightforward to save the Values of the
    //  already loaded modules because of lifetime challenges. Not a big deal, no need
    //  to cache. But we should at least track what has already been loaded / maintain
    //  some kind of stack in order to fail on cycles.
    // match self.modules.entry(path) {
    //     Entry::Occupied(e) => Ok(e.into_mut()),
    //     Entry::Vacant(e) => {
    //         let path = e.key();
    //         let source = self
    //             .finder
    //             .find_source_by_path(path)
    //             .with_context(|| format!("Couldn't find Lua module {path}"))?;
    //         let env = safe_lua.create_fresh_environment(true)?;
    //         let value = safe_lua
    //             .compile_and_execute("Module", source.as_ref(), env)
    //             .with_context(|| format!("Couldn't compile and execute Lua module {path}"))?;
    //         Ok(e.insert(value))
    //     }
    // }
}

fn create_require_function(
    finder: impl LuaModuleFinder + Clone + 'static,
    lua: &Lua,
) -> anyhow::Result<Function> {
    let require = lua.create_function_mut(move |lua, path: String| {
        let value = find_and_execute_module(finder.clone(), lua, &path).unwrap();
        Ok(value)
    })?;
    Ok(require)
}

#[derive(Clone)]
pub struct IncludedDirLuaModuleFinder {
    dir: Dir<'static>,
}

impl IncludedDirLuaModuleFinder {
    pub fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl LuaModuleFinder for IncludedDirLuaModuleFinder {
    fn find_source_by_path(&self, path: &str) -> Option<Cow<'static, str>> {
        let contents = self.dir.get_file(path)?.contents_utf8()?;
        Some(contents.into())
    }
}
