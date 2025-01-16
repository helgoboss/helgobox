use crate::domain::{compile_and_execute, create_fresh_environment};
use anyhow::{bail, Context};
use auto_impl::auto_impl;
use camino::{Utf8Path, Utf8PathBuf};
use include_dir::Dir;
use mlua::{Function, Lua, Value};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

/// Allows executing Lua code as a module that may require other modules.
pub struct LuaModuleContainer<F> {
    finder: Result<F, &'static str>,
    // modules: NonCryptoHashMap<String, Value<'a>>,
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
        normalized_path: Option<String>,
        display_name: String,
        code: &str,
    ) -> anyhow::Result<Value> {
        execute_as_module(
            lua,
            normalized_path,
            display_name,
            code,
            self.finder.clone(),
            SharedAccumulator::default(),
        )
    }
}

#[derive(Default)]
struct Accumulator {
    required_modules_stack: Vec<String>,
}

impl Accumulator {
    /// The given module must be normalized, i.e. it should contain the extension.
    pub fn push_module(&mut self, normalized_path: String) -> anyhow::Result<()> {
        let stack = &mut self.required_modules_stack;
        tracing::debug!(msg = "Pushing module onto stack", %normalized_path, ?stack);
        if stack.iter().any(|path| path == &normalized_path) {
            bail!("Detected cyclic Lua module dependency: {normalized_path}");
        }
        stack.push(normalized_path);
        Ok(())
    }

    pub fn pop_module(&mut self) {
        let stack = &mut self.required_modules_stack;
        tracing::debug!(msg = "Popping top module from stack", ?stack);
        stack.pop();
    }
}

type SharedAccumulator = Rc<RefCell<Accumulator>>;

fn find_and_execute_module<'lua>(
    lua: &'lua Lua,
    finder: impl LuaModuleFinder + Clone + 'static,
    accumulator: SharedAccumulator,
    required_path: &str,
) -> anyhow::Result<Value> {
    // Validate
    let root_info = || format!("\n\nModule root path: {}", finder.module_root_path());
    let path = Utf8Path::new(required_path);
    if path.is_absolute() {
        bail!("Required paths must not start with a slash. They are always relative to the preset sub directory.{}", root_info());
    }
    if path
        .components()
        .any(|comp| matches!(comp.as_str(), "." | ".."))
    {
        bail!("Required paths containing . or .. are forbidden. They are always relative to the preset sub directory.{}", root_info());
    }
    // Substitute preset runtime stub
    if lua_module_path_without_ext(path.as_str()) == LUA_PRESET_RUNTIME_NAME {
        let table = lua.create_table()?;
        let finder = finder.clone();
        let include_str = lua.create_function(move |_, path: String| {
            let content = finder
                .find_source_by_path(&path)
                .map(|content| content.to_string());
            Ok(content)
        })?;
        table.set("include_str", include_str)?;
        return Ok(Value::Table(table));
    }
    // Find module and get its source
    let (normalized_path, source) = if path
        .extension()
        .is_some_and(|ext| matches!(ext, "luau" | "lua"))
    {
        // Extension given. Just get file directly.
        let source = finder
            .find_source_by_path(path.as_str())
            .with_context(|| format!("Couldn't find Lua module [{path}].{}", root_info()))?;
        (path.to_string(), source)
    } else {
        // No extension given. Try ".luau" and ".lua".
        ["luau", "lua"]
            .into_iter()
            .find_map(|ext| {
                let path_with_extension = format!("{path}.{ext}");
                tracing::debug!(msg = "Finding module by path...", %path_with_extension);
                let source = finder.find_source_by_path(&path_with_extension)?;
                Some((path_with_extension, source))
            })
            .with_context(|| {
                format!(
                    "Couldn't find Lua module [{path}]. Tried with extension \".lua\" and \".luau\".{}", root_info()
                )
            })?
    };
    // Execute module
    execute_as_module(
        lua,
        Some(normalized_path.clone()),
        normalized_path,
        source.as_ref(),
        Ok(finder),
        accumulator,
    )
}

pub fn lua_module_path_without_ext(path: &str) -> &str {
    path.strip_suffix(".luau")
        .or_else(|| path.strip_suffix(".lua"))
        .unwrap_or(path)
}

fn execute_as_module<'lua>(
    lua: &'lua Lua,
    normalized_path: Option<String>,
    display_name: String,
    code: &str,
    finder: Result<impl LuaModuleFinder + Clone + 'static, &'static str>,
    accumulator: SharedAccumulator,
) -> anyhow::Result<Value> {
    let env = create_fresh_environment(lua, true)?;
    let require = create_require_function(lua, finder, accumulator.clone())?;
    env.set("require", require)?;
    let pop_later = if let Some(p) = normalized_path {
        accumulator.borrow_mut().push_module(p)?;
        true
    } else {
        false
    };
    let value = compile_and_execute(lua, display_name.clone(), code, env)
        .with_context(|| format!("Couldn't compile and execute Lua module {display_name}"))?;
    if pop_later {
        accumulator.borrow_mut().pop_module();
    }
    Ok(value)
    // TODO-medium-performance Instead of just detecting cycles, we could cache the module execution result and return
    //  it whenever it's queried again.
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
    lua: &'lua Lua,
    finder: Result<impl LuaModuleFinder + Clone + 'static, &'static str>,
    accumulator: SharedAccumulator,
) -> anyhow::Result<Function> {
    let require = lua.create_function_mut(move |lua, required_path: String| {
        let finder = finder.clone().map_err(mlua::Error::runtime)?;
        let value =
            find_and_execute_module(lua, finder.clone(), accumulator.clone(), &required_path)
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
    dir: Utf8PathBuf,
}

impl FsDirLuaModuleFinder {
    pub fn new(dir: Utf8PathBuf) -> Self {
        Self { dir }
    }
}

impl LuaModuleFinder for FsDirLuaModuleFinder {
    fn module_root_path(&self) -> String {
        self.dir.to_string()
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
        tracing::debug!(msg = "find_source_by_path", ?absolute_path);
        let content = fs::read_to_string(absolute_path).ok()?;
        tracing::debug!(msg = "find_source_by_path successful");
        Some(content.into())
    }
}

const LUA_PRESET_RUNTIME_NAME: &str = "preset_runtime";
