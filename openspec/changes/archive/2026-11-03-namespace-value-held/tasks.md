# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The walk

- [x] 2.1 `crates/core/src/query/walk/names.rs`: a use of a namespace, or of a local a `require` binds, alone is a use of a name only where the file reads it, and otherwise the file is held
- [x] 2.2 Through a namespace the name read is the member the walk asked about

## 3. Tests

- [x] 3.1 One test for each scenario of the delta, in `crates/core/tests/queries/names.rs`; the old tests follow the new answer, and the property that takes every import as used leaves a namespace out
- [x] 3.2 `crates/gsttt/tests/cli/impact.rs`: the TypeScript connector's namespace test follows the new answer

## 4. The wording

- [x] 4.1 `docs/architecture.md` and the doc comment of `Local::refers`
