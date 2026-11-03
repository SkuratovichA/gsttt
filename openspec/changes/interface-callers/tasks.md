# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/fact.rs`: `Symbols.overrides`
- [x] 2.2 `crates/core/src/graph/symbols.rs`: the overrides in the flat table and `FileSymbols::overrides`, with the round trip
- [x] 2.3 `crates/core/src/testing.rs`: `Code::overrides`
- [x] 2.4 `FACT_FORMAT` 17 to 18 and `LAYOUT` 11 to 12
- [x] 2.5 `crates/core/src/query/walk/overrides.rs`: the same member of each head of a class, through imports, barrels and namespaces
- [x] 2.6 `crates/core/src/query/walk/names.rs` and `crates/core/src/query/impact.rs`: the walk starts also from what the member overrides, with a note

## 3. The TypeScript connector

- [x] 3.1 The recorder: the members of an interface as declarations
- [x] 3.2 The recorder: `overrides` of a class
- [x] 3.3 `READER_VERSION` 26 to 27

## 4. The scenarios

- [x] 4.1 Queries: tests on a real tree for the seven scenarios
- [x] 4.2 TypeScript: tests for the two scenarios of the connector
