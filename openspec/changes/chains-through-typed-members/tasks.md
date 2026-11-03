# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. Core

- [x] 2.1 `FileSymbols::reads_on`, a read of one member on another without allocating
- [x] 2.2 The walk by name follows a member that holds a value through a chain of up to four names
- [x] 2.3 Members that hold each other and a crowded head end the walk
- [x] 2.4 `Symbols.holds` kept apart from `heirs`, the docs of `Symbols.reads`
- [x] 2.5 `FACT_FORMAT` 20 to 21 and `LAYOUT` 13 to 14

## 3. The Python connector

- [x] 3.1 Members that hold a value of a class: properties, fields and returns
- [x] 3.2 Reads of one member on another, none on a builtin value, and one on a name the file binds
- [x] 3.3 The connector `version` 6 to 7

## 4. Tests

- [x] 4.1 The scenarios of `queries` in `crates/core/tests/queries/names.rs`
- [x] 4.2 The scenarios of `python` in `crates/lang-python/tests/scenarios.rs` and `crates/gsttt/tests/cli/impact.rs`
- [x] 4.3 The changed Python snapshots, read and accepted
