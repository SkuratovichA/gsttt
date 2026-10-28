# Design

## Context

The config crate evaluates `.gsttt/init.lua` in a Lua 5.4 sandbox built on `mlua`. `sandbox.rs` and `sandbox/` build the restricted globals, the limits and the string library without patterns, `value.rs` turns Lua tables into JSON, `editor.rs` writes the LuaLS annotations, `.luarc.json` and the starter, and `pack.rs` evaluates a connector's built-in packs in the same sandbox. `source.rs` (reading a file safely), `external/` (connectors that run as a process), `fingerprint.rs` and `text.rs` (the 1 KiB cut) do not depend on Lua and stay.

## Goals

- The same `Setup` from data, with no code run to get it.
- Every error names the file, the line and the column.
- Editors complete and check the config and the packs through JSON Schemas generated from the registry.

## Non-goals

- Checking a module's options against its type while reading. They stay JSON in the setup, as with Lua, `gsttt config check` checks them, and the schema already catches a misspelt option in an editor.
- Conditions, functions or includes. A config that differs per machine is out of scope, because the index must not differ.
- Writing the config back. Nothing rewrites a user's file, so its comments and layout need no preserving.

## Decisions

### Files

```text
.gsttt/
├── config.toml          the config; without it every detected module runs with its defaults
├── packs/<name>.toml    one rule pack per file
└── schema/
    ├── config.json      written by gsttt init, named by the starter's #:schema line
    └── pack.json
```

The constants are `CONFIG_PATH`, `PACKS_PATH`, `CONFIG_SCHEMA_PATH` and `PACK_SCHEMA_PATH`, in place of `INIT_PATH`, `LUARC_PATH` and `ANNOTATIONS_PATH`.

### The config

What a call on the `gsttt` module did becomes a key or a table:

| Lua | TOML |
| --- | --- |
| `g.setup { exclude = {…}, http_prefixes = {…}, max_file_bytes = n, detect = b }` | the top-level keys `exclude`, `http_prefixes`, `max_file_bytes` and `detect` |
| `g.ts { tsconfig = "…" }` | a `[ts]` table; an empty `[ts]` turns the module on with no options |
| `g.disable "openapi"` | `disable = ["openapi"]` |
| `g.disable_pack "axios"` | `disable_packs = ["axios"]` |
| `g.external { … }` | an `[[external]]` entry, with `[[external.provides]]` and `[external.options]` |
| `g.pack { … }` | a file in `.gsttt/packs/` |

```toml
#:schema ./schema/config.json
exclude = ["fixtures/**"]
http_prefixes = ["/api"]
disable = ["openapi"]

[ts]
tsconfig = "tsconfig.base.json"
paths = { "@app/*" = ["src/*"] }

[[external]]
name = "kotlin"
cmd = ["gsttt-kotlin"]
files = ["**/*.kt"]
version = "1"
env = ["JAVA_HOME"]

[[external.provides]]
kind = "env"
role = "calls"
via = ["System.getenv"]
blind_spots = ["names built at run time"]

[external.options]
gradle = "build.gradle.kts"
```

### Reading a file

The config and each pack go through the same steps, and each step refuses what the next one cannot take:

1. `read_source` in `source.rs`: a regular UTF-8 file of at most 1 MiB that lies, once links are resolved, inside the repository and inside its bound. Today it checks only the bound, so the files of a `.gsttt/packs` that is itself a link to a folder outside the repository get through; it checks the repository root first, and the error says which of the two the file leaves.
2. The depth check, on toml_parser's events, before any value is built.
3. `toml::de::DeTable::parse`: the syntax, with spans.
4. A walk over the parsed values that refuses a date, a time and a float that is not finite, wherever it is. Nothing in the config or a pack takes one, and through serde a date would reach an options table as toml's private datetime map and a NaN as `null`.
5. Typed deserialization of each part, with `deny_unknown_fields`.

An error is a byte span and a message until it leaves the crate as `<file>:<line>:<column>: <message>`, cut by `text::cut` at 1 KiB. The line and the column come from `gsttt_core::text::LineIndex`, extended with a column counted in characters, rather than from a second line counter in this crate:

```rust
impl LineIndex {
    /// The 1-based column of `offset` on its line, in characters, so that it is the column an editor shows.
    pub fn column(&self, text: &str, offset: u32) -> u32 { … }
}
```

### The depth check

Deserializing a nested value recurses, in serde and in the `Drop` of the value. toml bounds part of that: its parser refuses inline tables and arrays nested deeper than 80, through toml_parser's `RecursionGuard`, and keys of 80 parts or more. The bounds add up, though, and each part of a header that names an array of tables adds a level of its own, so a file within them can still nest more than two hundred deep, which a small stack does not hold. `RecursionGuard` cannot do this check's counting, because it counts inline values only. The check runs on toml_parser's events, from `parser::parse_document` over the tokens of `Source::new(text).lex()`, and past the limit it returns false from `array_open` and `inline_table_open`, which makes the parser skip the value without descending into it. The check itself never goes deeper than the limit.

```rust
struct DepthCheck<'s> {
    source: &'s str,
    header: u32,                             // containers of the current [header]
    open: Vec<u32>,                          // depth of each open inline array or table
    key_parts: u32,                          // simple keys of the key being read
    pending: u32,                            // dotted parts of the last key, added to the value that follows
    path: Vec<String>,                       // decoded parts of the header being read
    arrays_of_tables: BTreeSet<Vec<String>>, // paths of the [[…]] headers seen so far
    passed: Option<Span>,                    // where the limit was first passed
}
```

- A header counts one per part, and one more for each part whose path is an array of tables, its last part included when the header is `[[…]]`. The path of a `[[…]]` header joins `arrays_of_tables` when the header closes. Parts are compared decoded, through toml_parser's key decoder, so that `"a"` and `a` are the same part.
- A dotted key adds its parts but the last to the value that follows it.
- An inline array or table opens one deeper than its container plus the pending dotted parts.
- The check ignores syntax errors, with `()` as its error sink: step 3 reports them with better messages, and both steps read the same events of the same parser.

### The top level

The modules' tables share the top level with the fixed keys, so the top level cannot be one struct with `deny_unknown_fields`. It is split by key:

```rust
for (key, value) in document.get_ref() {
    let name = key.get_ref().as_ref();
    if KEYS.contains(&name) {
        keys.insert(key.clone(), value.clone());
    } else if let Some(connector) = registry.iter().find(|connector| connector.name() == name) {
        modules.insert(connector.name(), module_options(name, value)?);
    } else {
        return Err(unknown_key(name, key.span(), registry));
    }
}
let keys = Keys::deserialize(Deserializer::from(Spanned::new(document.span(), keys)))?;
```

`Keys` is the struct of the fixed keys, `external` among them, with a doc comment on each field: the reader deserializes into it, and the schema takes its descriptions from it. A test pins `KEYS` to the properties of its schema. When an error's span lies inside the value of one of the keys, the message starts with that key, so that `max_file_bytes = "big"` names `max_file_bytes` even where toml's message does not. `disable` is checked against the registry's names afterwards; `disable_packs` is not, because packs come from connectors and from files the reader has not looked at yet. Modules enter the setup in the registry's order, whatever the order of their tables, and externals in the file's order.

### Options as JSON

A module's table and an external's `options` become `serde_json::Value` through serde, and their keys are then sorted at every level: the workspace builds `serde_json` with `preserve_order`, so a map keeps the order it was filled in. `canonical_json` in `crates/core/src/engine/keys.rs` sorts only the text it writes. An empty module table gives `Value::Null`, as an empty Lua table did, and a module given anything but a table fails with the module and what it was given.

### Packs

```toml
# Environment variables, read from the process or from the bundler.
name = "env"
language = "ts"

[[chains]]
root = ["process.env", "import.meta.env"]
kind = "env"
role = "calls"
segments = 1
```

- `read_pack` replaces `eval_pack`, and `builtin_packs` keeps its signature, so a connector's `PACKS` table changes only from `include_str!("../packs/<name>.lua")` to `.toml`.
- The user's packs are the files of `.gsttt/packs/` whose names end in `.toml` and do not start with `.`, read in the byte order of their names, each bound to `.gsttt/packs`. A pack's `name` must be its file name without `.toml`.
- `Pack` and its rules deny unknown fields already, so a misspelt rule key fails with its line in step 5.
- A pack that is a link to nothing fails with `is a link that leads nowhere`, where a config that is one counts as no config: a missing config means every detected module with its defaults, while a pack that the folder has just listed has no such meaning, and skipping it would drop its rules without a word.

A user's pack starts with `#:schema ../schema/pack.json`, the schema `gsttt init` writes. A built-in pack has no such line, because the repository of gsttt has no generated schema to point at.

Converting a pack must not change it. Before a `.lua` pack is touched, a snapshot test records the connector's `builtin_packs(PACKS)` as JSON; the packs are then rewritten as TOML, their comments carried over by hand, and the snapshot must pass unchanged. `toml::to_string` of the evaluated pack is a starting point for the text, not the result: the result reads like the Lua it replaces, with each rule list as an array of tables.

### Schemas

`DynConnector::options_schema` at `crates/core/src/connector.rs:287` takes the generator, so that every type is described once and referenced wherever it is used:

```rust
fn options_schema(&self, generator: &mut SchemaGenerator) -> Schema;

// for Erased<C>
fn options_schema(&self, generator: &mut SchemaGenerator) -> Schema {
    generator.subschema_for::<C::Options>()
}
```

`config_schema(registry)` and `pack_schema()` use `SchemaSettings::draft07()` with `option_add_null_type` off, because TOML has no null and an editor would otherwise offer one. The config schema is the schema of `Keys` with a property per module added and `additionalProperties` false, and its `definitions` are the generator's. `Provide` derives `JsonSchema`, and `ExternalSetup.provides` drops its `#[schemars(with = "Vec<serde_json::Value>")]`.

### The starter

```toml
#:schema ./schema/config.json
# The config of gsttt. Every key is described in schema/config.json, so an editor with a TOML language server completes and checks this file.
# Rule packs go in packs/<name>.toml, one per file, each starting with #:schema ../schema/pack.json.

# Detected in this repository. Each runs with its defaults; uncomment a table to give it options.
# [python]
# [ts]
```

With nothing detected, the last block says so and shows `# [ts]` as the way to turn a module on.

## What goes

`mlua`, and `memchr`, which only the sandbox's string library uses; `sandbox.rs` and `sandbox/`; `value.rs`; the annotations and `.luarc.json` in `editor.rs`; `ConfigError::Lua` and `ConfigError::Sandbox`; and the tests of the removed requirements. The Lua wording goes from `.editorconfig`, `AGENTS.md`, `README.md`, `docs/architecture.md`, the root `Cargo.toml` and the comments in `crates/core/src/connector.rs`, `crates/core/src/rules.rs`, `crates/core/src/setup.rs` and `crates/lang-ts/src/connector.rs`.

## Risks

- **No logic in the config.** A Lua config could compute a list; a TOML one writes it out. That is the point: the config is data, so the index is a function of the tree.
- **A pack's name is written twice**, as the file name and as `name`. A renamed file then fails loudly instead of renaming the pack without a word.
- **The text is read twice**, once to count the depth and once to parse it, both by toml_parser. A config is at most 1 MiB, and the second pass is the one that was always there.

## Order of work

1. The snapshot of the TypeScript packs, while they are Lua.
2. The reader, the config and the packs, next to the Lua code, with the TypeScript packs converted.
3. The schemas and the starter.
4. The Lua code removed and the docs updated.

When this change merges above the Python and Rust connectors, their packs are converted the same way, each with its snapshot taken first.
