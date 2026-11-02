# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/fact.rs`: `Heir` and `Symbols.heirs`
- [x] 2.2 `crates/core/src/graph/symbols.rs`: the heirs in the flat table and `FileSymbols::heirs`, with the round trip
- [x] 2.3 `crates/core/src/testing.rs`: `Code::heir`
- [x] 2.4 `LAYOUT` 8 to 9 and `FACT_FORMAT` 14 to 15
- [x] 2.5 `crates/core/src/query/walk/names.rs`: a member passes on to its heirs, in the importer, in the file of the member and among its callers

## 3. The TypeScript connector

- [x] 3.1 The recorder: the heads of a class and an interface, the class a top-level `const` holds, and `export default new Service()`
- [x] 3.2 The recorder: a member of a name with a heir pair, `this` of an inherited member, `super.member` and `super(...)`
- [x] 3.3 `READER_VERSION` 19 to 20

## 4. The scenarios

- [x] 4.1 Queries: tests for the seven scenarios of "Members that pass on"
- [x] 4.2 TypeScript: a fixture and tests for the five scenarios of "Heirs"
- [x] 4.3 CLI: one test on real TypeScript, with a subclass in the file of its base, a generated-style subclass and a constant that holds an instance

## 5. The docs

- [x] 5.1 `docs/architecture.md`: the heirs and the walk that passes a member on
