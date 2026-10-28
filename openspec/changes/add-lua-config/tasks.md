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
