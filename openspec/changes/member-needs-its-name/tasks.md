# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/fact.rs`: `Symbols.reads`
- [x] 2.2 `crates/core/src/graph/symbols.rs`: the reads in the flat table and `FileSymbols::reads_member`, with the round trip
- [x] 2.3 `crates/core/src/testing.rs`: `Code::reads`
- [x] 2.4 `LAYOUT` 9 to 10 and `FACT_FORMAT` 15 to 16
- [x] 2.5 `crates/core/src/query/walk/names.rs`: a use of a class alone, or of an heir alone, refers to a member only where the file reads its name, for the namespace locals and for `--changed`
- [x] 2.6 `crates/core/src/query/impact.rs`: the note that counts the files left out

## 3. The TypeScript connector

- [x] 3.1 The recorder: the reads of a member expression, a JSX member, a pattern key, an assignment target and the name a spy takes
- [x] 3.2 The recorder: `T['m']` as a use of `T.m`
- [x] 3.3 `READER_VERSION` 20 to 21

## 4. The scenarios

- [x] 4.1 Queries: tests for the six new scenarios and the changed ones of "Members called through a field" and "Members of a class"
- [x] 4.2 TypeScript: a fixture `symbols-reads` and tests for the three scenarios of "Reads of members"
- [x] 4.3 CLI: one test on real TypeScript, with a module that only lists a service and a class that calls the method through a typed field

## 5. The docs

- [x] 5.1 `docs/architecture.md` and `plugin/skills/gsttt/SKILL.md`: the reads, the rule and the note

## 6. Re-exports

- [x] 6.1 `crates/core/src/query/walk/names.rs`: a re-export of a class gets no row for a member, and `export * as ns` none for a name
- [x] 6.2 Queries: tests for the two new scenarios of "A star barrel names nothing"

## 7. The Go connector

- [x] 7.1 `crates/lang-go/src/walk.rs`, `analyze.rs` and `parse.rs`: the reads of a selector on a value of unknown type
- [x] 7.2 `READER_VERSION` 1 to 2
- [x] 7.3 Go: a test for the scenario of "Reads of members"
