# Tasks

## 1. Reading

- [x] 1.1 `read_source` checks the repository root before the bound, so a pack folder that links outside the repository is refused
- [x] 1.2 The depth check on toml_parser's events, an array of tables counting twice
- [x] 1.3 Errors as `<file>:<line>:<column>: <message>`, the column in characters through `LineIndex`, cut at 1 KiB
- [x] 1.4 Dates, times and floats that are not finite refused wherever they appear, with their key

## 2. The config

- [x] 2.1 `.gsttt/config.toml` into `Setup`: the fixed keys through `Keys`, a table per module in the registry's order, `[[external]]` in the file's order, `disable` checked against the registry
- [x] 2.2 An unknown top-level key named with the modules and the keys, and a value error named with its key
- [x] 2.3 Module tables and external options as JSON with sorted keys

## 3. Packs

- [x] 3.1 A snapshot of the TypeScript connector's built-in packs, taken while they are Lua
- [x] 3.2 `read_pack`, `builtin_packs` and `.gsttt/packs/*.toml`, the name checked against the file
- [x] 3.3 The ten TypeScript packs in TOML, with the snapshot unchanged
- [ ] 3.4 The packs of the Python and Rust connectors converted the same way, once this change merges above them

## 4. Editor support

- [x] 4.1 `DynConnector::options_schema` takes a shared `SchemaGenerator`, and `Provide` derives `JsonSchema`
- [x] 4.2 `config_schema` and `pack_schema`, draft-07, each type once under `definitions`
- [x] 4.3 The starter with its `#:schema` line, read back as the defaults
- [ ] 4.4 Every comment and doc comment that breaks a sentence or a paragraph across lines joined into one line, across the workspace (about 375 lines, most of them in `crates/core/src/rules.rs` and on `Role` in `crates/core/src/contract.rs`), with a test that no description in either schema breaks inside a paragraph; done at the top of the stack once this change merges there, so that the layers between do not conflict

## 5. Removal

- [x] 5.1 `mlua`, `memchr`, the sandbox, the Lua value conversion, the annotations and `.luarc.json` removed
- [x] 5.2 The docs, `AGENTS.md`, `README.md`, `.editorconfig` and the comments that name Lua updated

## 6. Proof

- [x] 6.1 A test annotated `#[scenario("config", …)]` for every scenario of this change's delta, and no test left for a removed requirement
- [x] 6.2 `cargo nextest run --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` and `openspec validate switch-config-to-toml --strict` pass
- [ ] 6.3 CI runs `taplo fmt --check` beside `cargo fmt --all --check`, added when this change merges above the change that adds CI

## 7. Review fixes

- [x] 7.1 A pack that is a link to nothing fails, where a config that is one stays no config
- [x] 7.2 No module schema offers `null`: `without_null` runs over the schema of each module, and a module without options gets an empty closed table
- [x] 7.3 `Provide` refuses an unknown key, so a misspelt `blind_spot` fails at the line where it is written
- [x] 7.4 A test pins that `read_source` checks the repository before the bound
- [x] 7.5 A file name or a path is made fit for a line in every message that holds one, so that it cannot steer the terminal
- [x] 7.6 `cut` never returns more than its limit, and leaves the mark out when it does not fit
- [x] 7.7 The maps of module options and of external options are sorted at every level by one function, which a test pins on a value written out of order, and the tests of evaluation compare a nested map
- [x] 7.8 The comment of the header loop of the depth check says what its stop bounds
- [x] 7.9 `mod evaluation` of the test crate is under `cfg(test)` like its siblings
- [x] 7.10 The path that `dotted` prints and the path of a typed error are pinned against each other, and `dotted` says where the other printer is
- [x] 7.11 The TOML files are formatted by taplo, and `.editorconfig` indents them by two
- [x] 7.12 The design says what the code does to take `null` out of the schemas, and the tasks name the follow-ups that wait for the stack above this change
