//! Evaluation of `.gsttt/init.lua` into a [`Setup`].
//!
//! The config sees one module, `require("gsttt")`, whose functions fill the setup as they run, and a `require` that loads only from `.gsttt/lua/`. Both are Rust callbacks inside the sandbox of [`crate::sandbox`], so a refused call fails with the file and line of the config that made it.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gsttt_core::Setup;
use gsttt_core::connector::DynConnector;
use gsttt_core::rules::Pack;
use gsttt_core::setup::{ExternalSetup, ModuleSetup};
use mlua::{FromLuaMulti, Function, IntoLuaMulti, Lua, Table, Value};
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::ConfigError;
use crate::external::{check_env, check_name, glob_set, options_object};
use crate::sandbox::{Sandbox, raise, text_chunk};
use crate::source::read_source;
use crate::value::to_json;

/// The config file, relative to the repository root. Also the chunk name, so that errors start with it.
pub const INIT_PATH: &str = ".gsttt/init.lua";

/// Where `require` looks, relative to the repository root.
pub(crate) const MODULES: &str = ".gsttt/lua";

/// Registry keys under which the callbacks find the gsttt module and the modules loaded so far. Lua values stay in Lua: a Rust closure that held a table of the state it is stored in would never be freed.
const MODULE_KEY: &str = "gsttt.module";
const LOADED_KEY: &str = "gsttt.loaded";

/// The functions of the gsttt module besides the one per module of the registry.
pub(crate) const FUNCTIONS: [&str; 5] = ["setup", "disable", "pack", "disable_pack", "external"];

/// What `g.setup` takes. Every key is optional and a key that is not given leaves the setup as it was.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetupOptions {
    /// Gitignore-style patterns of files to leave out, on top of what git ignores.
    pub(crate) exclude: Option<Vec<String>>,
    /// Path prefixes a gateway or proxy puts in front of routes, like `/api`: with it, a call to `/api/bookings` meets the route `/bookings`.
    pub(crate) http_prefixes: Option<Vec<String>>,
    /// Files above this size, in bytes, are counted in coverage and not parsed.
    pub(crate) max_file_bytes: Option<u64>,
    /// Turns on every module whose detection finds something in the repository. A module listed with options runs either way.
    pub(crate) detect: Option<bool>,
}

/// What the callbacks of one evaluation share.
struct Evaluation {
    root: PathBuf,
    /// The names of the registry, in its order.
    modules: Vec<String>,
    setup: RefCell<Setup>,
    /// The modules being loaded, outermost first, to name a cycle.
    loading: RefCell<Vec<String>>,
}

/// Reads `.gsttt/init.lua` under `root` and evaluates it in the sandbox into a [`Setup`]. A repository without that file gets [`Setup::default`].
///
/// `registry` lists the modules the binary ships: the gsttt module has one function per name in it, and a name outside it is an error that lists the known ones.
pub fn evaluate(root: &Path, registry: &[&dyn DynConnector]) -> Result<Setup, ConfigError> {
    let source = match read_source(root, INIT_PATH, None) {
        Ok(Some(source)) => source,
        Ok(None) => return Ok(Setup::default()),
        Err(error) => {
            return Err(ConfigError::Read {
                path: INIT_PATH.to_owned(),
                message: error.to_string(),
            });
        }
    };
    let sandbox = Sandbox::new().map_err(|error| ConfigError::Sandbox(error.to_string()))?;
    let state = Rc::new(Evaluation {
        root: root.to_path_buf(),
        modules: registry
            .iter()
            .map(|connector| connector.meta().name.to_owned())
            .collect(),
        setup: RefCell::new(Setup::default()),
        loading: RefCell::new(Vec::new()),
    });
    install(sandbox.lua(), &state).map_err(|error| ConfigError::Sandbox(error.to_string()))?;
    sandbox
        .load(INIT_PATH, &source)
        .exec()
        .map_err(|error| ConfigError::Lua(sandbox.explain(&error)))?;
    let setup = state.setup.borrow().clone();
    Ok(setup)
}

fn install(lua: &Lua, state: &Rc<Evaluation>) -> mlua::Result<()> {
    lua.set_named_registry_value(MODULE_KEY, gsttt_module(lua, state)?)?;
    lua.set_named_registry_value(LOADED_KEY, lua.create_table()?)?;
    lua.globals()
        .raw_set("require", callback(lua, state, require)?)
}

/// A Lua function that calls `body` with the state of the evaluation.
fn callback<A: FromLuaMulti + 'static, R: IntoLuaMulti + 'static>(
    lua: &Lua,
    state: &Rc<Evaluation>,
    body: fn(&Lua, &Evaluation, &A) -> mlua::Result<R>,
) -> mlua::Result<Function> {
    let state = Rc::clone(state);
    lua.create_function(move |lua, arguments: A| body(lua, &state, &arguments))
}

/// The table `require("gsttt")` returns. A key it does not have is an error and not `nil`, which is what makes a misspelt module loud.
fn gsttt_module(lua: &Lua, state: &Rc<Evaluation>) -> mlua::Result<Table> {
    let module = lua.create_table()?;
    module.raw_set("setup", callback(lua, state, setup)?)?;
    module.raw_set("disable", callback(lua, state, disable)?)?;
    module.raw_set("pack", callback(lua, state, pack)?)?;
    module.raw_set("disable_pack", callback(lua, state, disable_pack)?)?;
    module.raw_set("external", callback(lua, state, external)?)?;
    for name in &state.modules {
        let state = Rc::clone(state);
        let module_name = name.clone();
        let function = lua.create_function(move |lua, options: Value| {
            turn_on(lua, &state, &module_name, &options)
        })?;
        module.raw_set(name.as_str(), function)?;
    }
    let strict = lua.create_table()?;
    let known = Rc::clone(state);
    strict.raw_set(
        "__index",
        lua.create_function(
            move |lua, (_, key): (Table, Value)| -> mlua::Result<Value> {
                Err(raise(lua, missing_field(&known, &key)))
            },
        )?,
    )?;
    strict.raw_set(
        "__newindex",
        lua.create_function(
            |lua, (_, key, _): (Table, Value, Value)| -> mlua::Result<()> {
                Err(raise(
                    lua,
                    format!(
                        "gsttt is read-only, so `{}` cannot be set: call its functions instead",
                        key_name(&key)
                    ),
                ))
            },
        )?,
    )?;
    // Hidden from `getmetatable`: the config must not reach the table that Lua consults, to write a weak mode or a finalizer into it.
    strict.raw_set("__metatable", false)?;
    module.set_metatable(Some(strict))?;
    Ok(module)
}

fn key_name(key: &Value) -> String {
    match key {
        Value::String(text) => text.to_string_lossy(),
        other => format!("a {} key", other.type_name()),
    }
}

fn missing_field(state: &Evaluation, key: &Value) -> String {
    match key {
        Value::String(name) => unknown_module(state, &name.to_string_lossy()),
        other => format!("gsttt has no field for a {} key", other.type_name()),
    }
}

fn unknown_module(state: &Evaluation, name: &str) -> String {
    format!(
        "unknown module `{name}`: the known modules are {}, and the other functions of gsttt are {}",
        state.modules.join(", "),
        FUNCTIONS.join(", ")
    )
}

fn expect_table(lua: &Lua, function: &str, value: &Value) -> mlua::Result<()> {
    match value {
        Value::Table(_) => Ok(()),
        other => Err(raise(
            lua,
            format!("{function} takes a table, got {}", other.type_name()),
        )),
    }
}

fn expect_text(lua: &Lua, function: &str, what: &str, value: &Value) -> mlua::Result<String> {
    match value {
        Value::String(text) => text
            .to_str()
            .map(|text| String::from(&*text))
            .map_err(|_| raise(lua, format!("{function}: {what} must be valid UTF-8"))),
        other => Err(raise(
            lua,
            format!(
                "{function} takes {what} (a string), got {}",
                other.type_name()
            ),
        )),
    }
}

/// A table argument as `T`. An unknown key, a missing one or a wrong type is an error that names `function` and what serde found.
fn read<T: DeserializeOwned>(lua: &Lua, function: &str, value: &Value) -> mlua::Result<T> {
    expect_table(lua, function, value)?;
    let json = to_json(value, function).map_err(|error| raise(lua, error))?;
    serde_json::from_value(json).map_err(|error| raise(lua, format!("{function}: {error}")))
}

fn setup(lua: &Lua, state: &Evaluation, options: &Value) -> mlua::Result<()> {
    let options: SetupOptions = read(lua, "g.setup", options)?;
    let mut setup = state.setup.borrow_mut();
    if let Some(exclude) = options.exclude {
        setup.exclude = exclude;
    }
    if let Some(prefixes) = options.http_prefixes {
        setup.http_prefixes = prefixes;
    }
    if let Some(bytes) = options.max_file_bytes {
        setup.max_file_bytes = bytes;
    }
    if let Some(detect) = options.detect {
        setup.detect = detect;
    }
    Ok(())
}

/// `g.ts { … }`: turns the module on with its options. The options stay JSON here; the module checks them against its own schema when the engine opens it.
fn turn_on(lua: &Lua, state: &Evaluation, module: &str, options: &Value) -> mlua::Result<()> {
    let function = format!("g.{module}");
    let options = match options {
        Value::Nil => serde_json::Value::Null,
        Value::Table(_) => match to_json(options, &function).map_err(|error| raise(lua, error))? {
            // No options, and the defaults.
            serde_json::Value::Array(items) if items.is_empty() => serde_json::Value::Null,
            serde_json::Value::Array(_) => {
                return Err(raise(
                    lua,
                    format!("{function} takes named options, not a list"),
                ));
            }
            named => named,
        },
        other => {
            return Err(raise(
                lua,
                format!(
                    "{function} takes a table of options, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let mut setup = state.setup.borrow_mut();
    if setup.module(module).is_some() {
        return Err(raise(
            lua,
            format!("module `{module}` is already turned on by an earlier call of {function}"),
        ));
    }
    setup.modules.push(ModuleSetup {
        module: module.to_owned(),
        options,
    });
    Ok(())
}

fn disable(lua: &Lua, state: &Evaluation, name: &Value) -> mlua::Result<()> {
    let name = expect_text(lua, "g.disable", "a module name", name)?;
    if !state.modules.contains(&name) {
        return Err(raise(lua, unknown_module(state, &name)));
    }
    let mut setup = state.setup.borrow_mut();
    if !setup.disable.contains(&name) {
        setup.disable.push(name);
    }
    Ok(())
}

/// The built-in packs live in the connectors, so a name that matches none of them cannot be told from a typo here.
fn disable_pack(lua: &Lua, state: &Evaluation, name: &Value) -> mlua::Result<()> {
    let name = expect_text(lua, "g.disable_pack", "a pack name", name)?;
    let mut setup = state.setup.borrow_mut();
    if !setup.disable_packs.contains(&name) {
        setup.disable_packs.push(name);
    }
    Ok(())
}

fn pack(lua: &Lua, state: &Evaluation, table: &Value) -> mlua::Result<()> {
    expect_table(lua, "g.pack", table)?;
    let json = to_json(table, "g.pack").map_err(|error| raise(lua, error))?;
    let name = json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let pack: Pack = serde_json::from_value(json).map_err(|error| {
        raise(
            lua,
            match &name {
                Some(name) => format!("pack `{name}`: {error}"),
                None => format!("g.pack: {error}"),
            },
        )
    })?;
    let mut setup = state.setup.borrow_mut();
    if setup.packs.iter().any(|added| added.name == pack.name) {
        return Err(raise(lua, format!("pack `{}` is added twice", pack.name)));
    }
    setup.packs.push(pack);
    Ok(())
}

fn external(lua: &Lua, state: &Evaluation, table: &Value) -> mlua::Result<()> {
    let mut external: ExternalSetup = read(lua, "g.external", table)?;
    check_external(state, &mut external)
        .map_err(|message| raise(lua, format!("g.external: {message}")))?;
    state.setup.borrow_mut().externals.push(external);
    Ok(())
}

/// What serde cannot check: the name is usable as a module name, the command, the globs, the environment names and the options make sense. The options are left as the object they mean: a table without keys is read as an empty list, and the setup of an external without options must not depend on how the config wrote it.
fn check_external(state: &Evaluation, external: &mut ExternalSetup) -> Result<(), String> {
    check_name(&external.name).map_err(|error| format!("the name `{}` {error}", error.shown()))?;
    let name = &external.name;
    if state.modules.contains(name) {
        return Err(format!("the name `{name}` is already a module of gsttt"));
    }
    if state
        .setup
        .borrow()
        .externals
        .iter()
        .any(|declared| declared.name == *name)
    {
        return Err(format!("`{name}` is declared twice"));
    }
    if external.cmd.first().is_none_or(String::is_empty) {
        return Err(format!(
            "`{name}` needs a `cmd`: the program and its arguments"
        ));
    }
    if external.files.is_empty() {
        return Err(format!(
            "`{name}` needs `files`: the globs of the files it reads"
        ));
    }
    glob_set(&external.files).map_err(|error| format!("`files` of `{name}` {error}"))?;
    check_env(&external.env).map_err(|error| format!("`env` of `{name}` {error}"))?;
    external.options = options_object(&external.options)
        .map(serde_json::Value::Object)
        .map_err(|error| format!("`options` of `{name}` {error}"))?;
    Ok(())
}

/// `require(name)`: the gsttt module, or a file under `.gsttt/lua/`, evaluated once per evaluation.
fn require(lua: &Lua, state: &Evaluation, name: &Value) -> mlua::Result<Value> {
    let name = expect_text(lua, "require", "a module name", name)?;
    if name == "gsttt" {
        return lua
            .named_registry_value::<Table>(MODULE_KEY)
            .map(Value::Table);
    }
    let Some(relative) = module_path(&name) else {
        return Err(raise(
            lua,
            format!(
                "module `{name}` cannot be loaded: a name is dotted parts that each name a file or a folder under {MODULES}, without `..`, slashes or empty parts"
            ),
        ));
    };
    let loaded: Table = lua.named_registry_value(LOADED_KEY)?;
    let cached: Value = loaded.raw_get(name.as_str())?;
    if !cached.is_nil() {
        return Ok(cached);
    }
    if state.loading.borrow().contains(&name) {
        let chain = state.loading.borrow().join(", ");
        return Err(raise(
            lua,
            format!("module `{name}` requires itself (loading {chain})"),
        ));
    }
    let Some((file, source)) = find_module(lua, state, &name, &relative)? else {
        return Err(raise(
            lua,
            format!(
                "module `{name}` not found; tried {MODULES}/{relative}.lua and {MODULES}/{relative}/init.lua"
            ),
        ));
    };
    state.loading.borrow_mut().push(name.clone());
    let result = text_chunk(lua, &file, &source).call::<Value>(name.as_str());
    state.loading.borrow_mut().pop();
    // As in Lua's own `require`, a module that returns nothing counts as `true`.
    let value = match result? {
        Value::Nil => Value::Boolean(true),
        other => other,
    };
    loaded.raw_set(name.as_str(), value.clone())?;
    Ok(value)
}

/// `a.b` as `a/b`. A name with an empty part (`..`, a leading or trailing dot), a slash, a colon or a NUL byte is not a module name, which also keeps every name inside the folder.
fn module_path(name: &str) -> Option<String> {
    let valid = name
        .split('.')
        .all(|part| !part.is_empty() && !part.contains(['/', '\\', ':', '\0']));
    valid.then(|| name.replace('.', "/"))
}

/// The first of `<relative>.lua` and `<relative>/init.lua` that exists, as its display path and its text. A file that is not a regular file inside `.gsttt/lua` once its links are resolved is refused, and so is one that is too large.
fn find_module(
    lua: &Lua,
    state: &Evaluation,
    name: &str,
    relative: &str,
) -> mlua::Result<Option<(String, String)>> {
    for file in [
        format!("{MODULES}/{relative}.lua"),
        format!("{MODULES}/{relative}/init.lua"),
    ] {
        match read_source(&state.root, &file, Some(MODULES)) {
            Ok(Some(source)) => return Ok(Some((file, source))),
            Ok(None) => {}
            Err(error) => {
                return Err(raise(
                    lua,
                    format!("cannot read module `{name}` at {file}: {error}"),
                ));
            }
        }
    }
    Ok(None)
}
