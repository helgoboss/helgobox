use crate::domain::{compile_and_execute, create_fresh_environment};
use anyhow::Context;
use camino::Utf8Path;
use include_dir::Dir;
use mlua::{Function, Lua, Table, Value};
use std::borrow::Cow;
use std::path::Path;

pub struct LuaModuleContainer<F> {
    finder: F,
    // modules: HashMap<String, Value<'a>>,
}

pub trait LuaModuleFinder {
    fn find_source_by_path(&self, path: &str) -> Option<Cow<str>>;
}

impl<F: LuaModuleFinder + 'static> LuaModuleContainer<F> {
    pub fn new(finder: F) -> Self {
        Self {
            finder,
            // modules: Default::default(),
        }
    }

    pub fn install_to(self, env: &Table, lua: &Lua) -> anyhow::Result<()> {
        let require = self.create_require_function(lua)?;
        env.set("require", require)?;
        Ok(())
    }

    pub fn create_require_function(mut self, lua: &Lua) -> anyhow::Result<Function> {
        let require = lua.create_function_mut(move |lua, path: String| {
            let value = self.get_module(lua, path).unwrap();
            Ok(value)
        })?;
        Ok(require)
    }

    pub fn get_module<'a, 'b>(
        &'a mut self,
        lua: &'b Lua,
        path: String,
    ) -> anyhow::Result<Value<'b>> {
        let source = self
            .finder
            .find_source_by_path(&path)
            .with_context(|| format!("Couldn't find Lua module {path}"))?;
        let env = create_fresh_environment(lua, true)?;
        let value = compile_and_execute(lua, "Module", source.as_ref(), env)
            .with_context(|| format!("Couldn't compile and execute Lua module {path}"))?;
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
}

pub struct IncludedDirLuaModuleFinder {
    dir: Dir<'static>,
}

impl IncludedDirLuaModuleFinder {
    pub fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl LuaModuleFinder for IncludedDirLuaModuleFinder {
    fn find_source_by_path(&self, path: &str) -> Option<Cow<str>> {
        let path = Utf8Path::new(path);
        let file = if path.extension().is_some() {
            // Extension given. Just get file directly.
            self.dir.get_file(path)?
        } else {
            // No extension given. Try ".luau" and ".lua".
            ["luau", "lua"]
                .into_iter()
                .find_map(|ext| self.dir.get_file(path.with_extension(ext)))?
        };
        let contents = file.contents_utf8()?;
        Some(contents.into())
    }
}
