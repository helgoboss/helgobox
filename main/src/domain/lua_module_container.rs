use crate::domain::{compile_and_execute, create_fresh_environment};
use anyhow::{bail, Context};
use auto_impl::auto_impl;
use camino::Utf8Path;
use include_dir::Dir;
use mlua::{Function, Lua, Value};
use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

/// Allows executing Lua code as a module that may require other modules.
pub struct LuaModuleContainer<F> {
    finder: Result<F, &'static str>,
    // modules: HashMap<String, Value<'a>>,
}

/// Trait for resolving Lua modules.
#[auto_impl(Rc)]
pub trait LuaModuleFinder {
    /// Returns a short information that let's the user know what's the root of the module tree (e.g. a path).
    fn module_root_path(&self) -> String;

    /// Returns the source of the Lua module at the given path or `None` if Lua module not found or doesn't have
    /// UTF8-encoded content.
    ///
    /// Requirements:
    ///
    /// - The passed path must not start with a slash.
    /// - The passed path should not contain .. or . components. If they do, behavior is undefined.
    fn find_source_by_path(&self, path: &str) -> Option<Cow<'static, str>>;
}

impl<F> LuaModuleContainer<F>
where
    F: LuaModuleFinder + Clone + 'static,
{
    /// Creates the module container using the given module finder.
    ///
    /// If you pass `None`, executing Lua code will still work but any usage of `require` will yield a readable error
    /// message. This way, we can inform users in scenarios where `require` intentionally is not allowed.
    pub fn new(finder: Result<F, &'static str>) -> Self {
        Self { finder }
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

fn find_and_execute_module<'lua>(
    finder: impl LuaModuleFinder + Clone + 'static,
    lua: &'lua Lua,
    path: &str,
) -> anyhow::Result<Value<'lua>> {
    let root_info = || format!("\n\nModule root path: {}", finder.module_root_path());
    let path = Utf8Path::new(path);
    if path.is_absolute() {
        bail!("Required paths must not start with a slash. They are always relative to the preset sub directory.{}", root_info());
    }
    if path
        .components()
        .any(|comp| matches!(comp.as_str(), "." | ".."))
    {
        bail!("Required paths containing . or .. are forbidden. They are always relative to the preset sub directory.{}", root_info());
    }
    let source = if path.extension().is_some() {
        // Extension given. Just get file directly.
        finder
            .find_source_by_path(path.as_str())
            .with_context(|| format!("Couldn't find Lua module [{path}].{}", root_info()))?
    } else {
        // No extension given. Try ".luau" and ".lua".
        ["luau", "lua"]
            .into_iter()
            .find_map(|ext| finder.find_source_by_path(path.with_extension(ext).as_str()))
            .with_context(|| {
                format!(
                    "Couldn't find Lua module [{path}]. Tried with extension \".lua\" and \".luau\".{}", root_info()
                )
            })?
    };
    execute_as_module(path.as_str(), source.as_ref(), Ok(finder), lua)
}

fn execute_as_module<'lua>(
    name: &str,
    code: &str,
    finder: Result<impl LuaModuleFinder + Clone + 'static, &'static str>,
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

fn create_require_function<'lua>(
    finder: Result<impl LuaModuleFinder + Clone + 'static, &'static str>,
    lua: &'lua Lua,
) -> anyhow::Result<Function<'lua>> {
    let require = lua.create_function_mut(move |lua, path: String| {
        let finder = finder.clone().map_err(mlua::Error::runtime)?;
        let value = find_and_execute_module(finder.clone(), lua, &path)
            .map_err(|e| mlua::Error::runtime(format!("{e:#}")))?;
        Ok(value)
    })?;
    Ok(require)
}

/// Files Lua modules within a specified binary-included directory.
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
    fn module_root_path(&self) -> String {
        "factory:/".to_string()
    }

    fn find_source_by_path(&self, path: &str) -> Option<Cow<'static, str>> {
        let contents = self.dir.get_file(path)?.contents_utf8()?;
        Some(contents.into())
    }
}

/// Files Lua modules within a specified file-system directory.
#[derive(Clone)]
pub struct FsDirLuaModuleFinder {
    dir: PathBuf,
}

impl FsDirLuaModuleFinder {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl LuaModuleFinder for FsDirLuaModuleFinder {
    fn module_root_path(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }

    fn find_source_by_path(&self, path: &str) -> Option<Cow<'static, str>> {
        let path = Utf8Path::new(path);
        // It's a precondition by contract that the given path is not absolute. However, in order to fail
        // fast in case this precondition is missed, we check again here. Because on a file system, absolute
        // files can actually work, but we don't want it to work.
        if path.is_absolute() {
            return None;
        }
        let absolute_path = self.dir.join(path);
        let content = fs::read_to_string(absolute_path).ok()?;
        Some(content.into())
    }
}
