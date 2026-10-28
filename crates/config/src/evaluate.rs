//! The config of a repository: `.gsttt/config.toml` and the packs in `.gsttt/packs/`, read into a [`Setup`].
//!
//! Nothing in them is run. The top level of the config holds a fixed set of keys ([`Keys`]), a table for each module of the registry, which turns the module on with the table as its options, and the `[[external]]` entries. serde reads the keys, but only the registry knows the names of the modules, so the top level is sorted out here before anything is read into a type. A run reports one error, the first of these steps to fail: the syntax, an unknown key, an unknown module in `disable`, a value of the wrong type, the options of a module, and the declaration of an external.

use std::fs;
use std::io;
use std::ops::Range;
use std::path::Path;

use gsttt_core::Setup;
use gsttt_core::connector::DynConnector;
use gsttt_core::rules::Pack;
use gsttt_core::setup::{ExternalSetup, ModuleSetup};
use schemars::JsonSchema;
use serde::Deserialize;
use toml::Spanned;
use toml::de::{DeTable, DeValue, ValueDeserializer};

use crate::document::{Located, parse, read_typed};
use crate::error::ConfigError;
use crate::external::{check_env, check_name, glob_set, options_object};
use crate::pack::read_pack;
use crate::source::read_source;

/// The config file, relative to the repository root. Also how errors name it.
pub const CONFIG_PATH: &str = ".gsttt/config.toml";

/// The folder of the user's rule packs, relative to the repository root.
pub const PACKS_PATH: &str = ".gsttt/packs";

/// The keys of the top level that are not modules, which is what [`Keys`] holds.
pub(crate) const KEYS: [&str; 7] = [
    "exclude",
    "http_prefixes",
    "max_file_bytes",
    "detect",
    "disable",
    "disable_packs",
    "external",
];

/// The keys of the top level that are not modules. A key that is left out keeps the default of [`Setup`].
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Keys {
    /// Gitignore-style patterns of files to leave out, on top of what git ignores.
    pub(crate) exclude: Vec<String>,
    /// Path prefixes a gateway or proxy puts in front of routes, like `/api`: with it, a call to `/api/bookings` meets the route `/bookings`.
    pub(crate) http_prefixes: Vec<String>,
    /// Files above this size, in bytes, are counted in coverage and not parsed.
    pub(crate) max_file_bytes: Option<u64>,
    /// Turns on every module whose detection finds something in the repository. A module with a table runs either way.
    pub(crate) detect: Option<bool>,
    /// Modules that stay off even when detected or given a table.
    pub(crate) disable: Vec<String>,
    /// Built-in packs to leave out, by name.
    pub(crate) disable_packs: Vec<String>,
    /// Connectors that run as a command speaking JSON lines, for languages gsttt does not ship.
    pub(crate) external: Vec<ExternalSetup>,
}

/// Reads `.gsttt/config.toml` and the packs in `.gsttt/packs/` under `root` into a [`Setup`]. A repository without them gets [`Setup::default`].
///
/// `registry` lists the modules the binary ships: a top-level table is a module when its name is in it, and any other name that is not a key of the config is an error that lists the known ones.
pub fn evaluate(root: &Path, registry: &[&dyn DynConnector]) -> Result<Setup, ConfigError> {
    let mut setup = match read_text(root, CONFIG_PATH, None)? {
        Some(text) => read_config(&text, registry)
            .map_err(|error| ConfigError::Invalid(error.render(CONFIG_PATH, &text)))?,
        None => Setup::default(),
    };
    setup.packs = read_packs(root)?;
    Ok(setup)
}

/// The text of `relative` under `root`, with a failure named after the file.
fn read_text(
    root: &Path,
    relative: &str,
    bound: Option<&str>,
) -> Result<Option<String>, ConfigError> {
    read_source(root, relative, bound).map_err(|error| ConfigError::Read {
        path: relative.to_owned(),
        message: error.to_string(),
    })
}

/// The packs of `.gsttt/packs/` in the order of their file names: each file that ends in `.toml` and does not start with a dot. Other files are not opened. A repository without the folder has none.
fn read_packs(root: &Path) -> Result<Vec<Pack>, ConfigError> {
    let unreadable = |error: io::Error| ConfigError::Read {
        path: PACKS_PATH.to_owned(),
        message: error.to_string(),
    };
    let entries = match fs::read_dir(root.join(PACKS_PATH)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(unreadable(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let name = entry.map_err(unreadable)?.file_name();
        if let Some(name) = name.to_str()
            && Path::new(name)
                .extension()
                .is_some_and(|extension| extension == "toml")
            && !name.starts_with('.')
        {
            names.push(name.to_owned());
        }
    }
    names.sort_unstable();

    let mut packs = Vec::with_capacity(names.len());
    for name in names {
        let file = format!("{PACKS_PATH}/{name}");
        // A link that leads nowhere is no file, as it is for the config.
        let Some(text) = read_text(root, &file, Some(PACKS_PATH))? else {
            continue;
        };
        packs.push(read_pack(&file, &text).map_err(|error| ConfigError::Invalid(error.message))?);
    }
    Ok(packs)
}

/// A top-level table named like a module of the registry.
struct ModuleTable<'text> {
    /// Where the module is in the registry, which is the order the setup lists it in.
    position: usize,
    name: &'static str,
    value: Spanned<DeValue<'text>>,
}

/// What the top level of the config holds, told apart.
struct TopLevel<'text> {
    /// The keys of [`KEYS`], as the table [`Keys`] is read from.
    keys: DeTable<'text>,
    modules: Vec<ModuleTable<'text>>,
    /// Where each `[[external]]` entry starts in the file, in the file's order.
    externals: Vec<Range<usize>>,
}

fn read_config(text: &str, registry: &[&dyn DynConnector]) -> Result<Setup, Located> {
    let names: Vec<&'static str> = registry
        .iter()
        .map(|connector| connector.meta().name)
        .collect();
    let document = parse(text)?;
    let span = document.span();
    let TopLevel {
        keys,
        mut modules,
        externals,
    } = sort_out(document.into_inner(), &names)?;
    check_disable(&keys, &names)?;
    let keys: Keys = read_typed(Spanned::new(span, keys))?;

    let defaults = Setup::default();
    let mut setup = Setup {
        detect: keys.detect.unwrap_or(defaults.detect),
        max_file_bytes: keys.max_file_bytes.unwrap_or(defaults.max_file_bytes),
        exclude: keys.exclude,
        http_prefixes: keys.http_prefixes,
        disable: keys.disable,
        disable_packs: keys.disable_packs,
        ..defaults
    };
    // The registry's order and not the file's: moving a table must not change the setup.
    modules.sort_by_key(|module| module.position);
    for ModuleTable { name, value, .. } in modules {
        setup.modules.push(ModuleSetup {
            module: name.to_owned(),
            options: module_options(name, value)?,
        });
    }
    for (mut external, span) in keys.external.into_iter().zip(externals) {
        check_external(&mut external, &names, &setup.externals)
            .map_err(|message| Located::new(message, span))?;
        setup.externals.push(external);
    }
    Ok(setup)
}

/// Splits the top level into the keys, the tables of the modules and the places of the externals. A key that is none of them is an error, and when there are several it is the first in the file that is named.
fn sort_out<'text>(
    table: DeTable<'text>,
    modules: &[&'static str],
) -> Result<TopLevel<'text>, Located> {
    let mut top = TopLevel {
        keys: DeTable::new(),
        modules: Vec::new(),
        externals: Vec::new(),
    };
    let mut unknown: Option<(Range<usize>, String)> = None;
    for (key, value) in table {
        let name: &str = key.get_ref();
        if let Some((position, module)) = modules
            .iter()
            .copied()
            .enumerate()
            .find(|(_, module)| *module == name)
        {
            top.modules.push(ModuleTable {
                position,
                name: module,
                value,
            });
        } else if KEYS.contains(&name) {
            if name == "external"
                && let DeValue::Array(entries) = value.get_ref()
            {
                top.externals = entries.iter().map(Spanned::span).collect();
            }
            top.keys.insert(key, value);
        } else if unknown
            .as_ref()
            .is_none_or(|(first, _)| key.span().start < first.start)
        {
            unknown = Some((key.span(), name.to_owned()));
        }
    }
    match unknown {
        Some((span, name)) => Err(Located::new(
            format!(
                "unknown key `{name}`: the known modules are {} and the known keys are {}",
                modules.join(", "),
                KEYS.join(", ")
            ),
            span,
        )),
        None => Ok(top),
    }
}

/// A name in `disable` that is no module of the registry, at the name in the file. Whether `disable` is a list of strings is for reading it into [`Keys`] to say.
fn check_disable(keys: &DeTable<'_>, modules: &[&str]) -> Result<(), Located> {
    let Some((_, disable)) = keys.iter().find(|(key, _)| {
        let name: &str = key.get_ref();
        name == "disable"
    }) else {
        return Ok(());
    };
    let DeValue::Array(items) = disable.get_ref() else {
        return Ok(());
    };
    let unknown = items.iter().find_map(|item| match item.get_ref() {
        DeValue::String(name) if !modules.contains(&&**name) => Some((name, item.span())),
        _ => None,
    });
    match unknown {
        Some((name, span)) => Err(Located::new(
            format!(
                "unknown module `{name}`: the known modules are {}",
                modules.join(", ")
            ),
            span,
        )),
        None => Ok(()),
    }
}

/// The options a module's table stands for: none for an empty table, which turns the module on with its defaults, and otherwise the table as JSON. The keys of every map are sorted, so that the order they were written in is not part of the setup.
fn module_options(name: &str, table: Spanned<DeValue<'_>>) -> Result<serde_json::Value, Located> {
    match table.get_ref() {
        DeValue::Table(options) if options.is_empty() => Ok(serde_json::Value::Null),
        DeValue::Table(_) => {
            let mut options = serde_json::Value::deserialize(ValueDeserializer::from(table))
                .map_err(Located::from)?;
            options.sort_all_objects();
            Ok(options)
        }
        other => Err(Located::new(
            format!(
                "`{name}` must be a table of options, like `[{name}]`, but it is of type {}",
                other.type_str()
            ),
            table.span(),
        )),
    }
}

/// What serde cannot check of an external: the name is usable as a module name and is not taken, the command, the globs, the environment names and the options make sense. The options become a map with its keys sorted, like those of a module.
fn check_external(
    external: &mut ExternalSetup,
    modules: &[&str],
    earlier: &[ExternalSetup],
) -> Result<(), String> {
    check_name(&external.name).map_err(|error| format!("the name `{}` {error}", error.shown()))?;
    let name = &external.name;
    if modules.contains(&name.as_str()) {
        return Err(format!("the name `{name}` is already a module of gsttt"));
    }
    if earlier.iter().any(|declared| declared.name == *name) {
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
    external.options.sort_all_objects();
    Ok(())
}
