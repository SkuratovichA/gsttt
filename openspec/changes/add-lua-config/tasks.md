# Tasks

## 1. Evaluation

- [x] 1.1 The sandbox: libraries, removed globals, `print` to the log, memory and instruction limits, ordered `pairs`; built-in packs moved onto it
- [x] 1.2 `require("gsttt")` with `setup`, module functions from a registry list, `disable`, `pack`, `disable_pack`, `external`, and strict keys
- [x] 1.3 Local `require` from `.gsttt/lua/`, tables to JSON, errors with file and line, the setup fingerprint

## 2. Editor support

- [x] 2.1 LuaLS annotations from option schemas, `.luarc.json`, the starter config

## 3. External connectors

- [x] 3.1 The `Connector` over a process: lazy start, the message loop, the timeout, failure of unanswered files
- [x] 3.2 The JSON schema of the messages

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("config", …)]` for every scenario of the `config` spec, the external ones against a scripted fake process
- [x] 4.2 `cargo nextest run -p gsttt-config`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 An external's process starts with a cleared environment: a short allowlist and the names its `env` lists
- [x] 5.2 Tables handed to the gsttt module are converted under a budget of a million values
- [x] 5.3 The `string` library has no pattern matcher and its `find` looks only for the text it is given
- [x] 5.4 A run stops after two seconds, `print` and `warn` write a bounded plain log, and an error message is cut at 1 KiB
- [x] 5.5 `next` is gone, and no address reaches a config through `tostring` or `string.format`
- [x] 5.6 The standard error of an external's process reaches the log as lines of at most 4 KiB without control characters
- [x] 5.7 The config and its modules are read through one function that resolves links, refuses what is not a regular file inside its folder, never waits on a pipe and stops at 1 MiB
- [x] 5.8 An external's process runs in a group of its own and the whole group is killed when it is stopped or has left
- [x] 5.9 `g.external` takes `provides` and `options`: the provides are known without the process, the options reach `open`, and `ready` no longer carries provides
- [x] 5.10 A `resolution` repeats the `from` and the `import` it answers, and one that answers another import is out of turn
- [x] 5.11 The name of an external is checked by one function, in the config and where the connector is built, and is at most 64 characters
- [x] 5.12 Tests for the gaps the review named: the line limit, a pipe that closes mid-line, threads that share one process and a fingerprint that follows module and external options
- [x] 5.13 An import of a file parsed before the process stopped is unresolved with the reason `unsupported` and the failure message as its detail
- [x] 5.14 The paths in `.luarc.json` are relative to `.gsttt/`, the folder LuaLS reads it in, and the starter `init.lua` says to open that folder as the workspace
- [x] 5.15 The proposal names what the fixes reached beyond the crate: `ExternalSetup` in core, and the `rustix` and `memchr` dependencies that make the crate unix-only
- [x] 5.16 The library calls whose native loop the script sizes are bounded before they run: `table.move`, `table.concat`, `table.insert`, `table.remove` and `table.sort` refuse a range or a length past a million elements and a `__len`, `string.rep` of nothing returns at once, and `setmetatable` refuses `__mode` as it refuses `__gc`
- [x] 5.17 The architecture document shows the external connector as it is configured and spoken (`env`, `provides` and `options` in `g.external`, a `ready` without `provides`, a `resolution` that echoes `from` and `import`) and names `.gsttt/` as the editor workspace
