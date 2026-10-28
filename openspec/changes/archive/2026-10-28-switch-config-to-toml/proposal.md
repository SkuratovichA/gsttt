# Proposal

## Why

The Lua config stays deterministic and safe only through code: a sandbox, limits on memory, instructions and time, a string library without patterns, guarded metatables and a fixed `pairs` order, and each of them can be wrong. What a repository tells gsttt is data: which modules run, their options, a few rule packs and the connectors that run as a command. TOML holds that as data, it is the format of `Cargo.toml` and `pyproject.toml` that users already write, and editors complete and check it against a JSON Schema.

## What Changes

- **BREAKING** The config is `.gsttt/config.toml`: top-level keys for the setup, a table per module and `[[external]]` entries. `.gsttt/init.lua` is no longer read.
- **BREAKING** Rule packs are TOML files: the user's in `.gsttt/packs/*.toml`, the built-in ones in each connector's `packs/*.toml`. `.gsttt/lua/` is no longer read.
- Errors name the file, the line and the column, and a nesting limit of 64 is checked before a file is parsed into values.
- Draft-07 JSON Schemas of the config and of a pack, generated from the registry, and a starter config whose `#:schema` line names the config's schema.
- `mlua`, the sandbox, the LuaLS annotations and `.luarc.json` are removed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `config`: the config and the packs are TOML, read as data and checked against generated JSON Schemas instead of evaluated in a Lua sandbox.

## Impact

`crates/config/` drops `mlua` and the sandbox and depends on `toml` and `toml_parser`. `DynConnector::options_schema` in `crates/core/src/connector.rs` takes a shared `SchemaGenerator`, `Provide` derives `JsonSchema`, and `LineIndex` in `crates/core/src/text.rs` gives a column. The ten packs in `crates/lang-ts/packs/` become TOML, and the packs of the Python and Rust connectors are converted in this change once it merges above them. The CLI change writes `config.toml` and the two schemas in `gsttt init` instead of `init.lua`, `.luarc.json` and the annotations.
