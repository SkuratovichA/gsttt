# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The reader

- [x] 2.1 `crates/core/src/fact.rs`: `ImportFlags::REPLACES` and `ImportFlags::ACTUAL`
- [x] 2.2 `crates/lang-ts/src/analyze/mocks.rs` and `crates/lang-ts/src/analyze.rs`: a `vi.mock` or `jest.mock` call that is a statement of its own at the top level, with a factory that does not use its first parameter, marks its import as replacing; the dynamic import of `vi.importActual` and `jest.requireActual` is marked as asking for the actual module
- [x] 2.3 `crates/lang-ts/src/project.rs`: `READER_VERSION` 9 to 10
- [x] 2.4 Fixtures, tests and snapshots of the five scenarios of `Mocks that replace a module` and of the scenario of `Actual modules` that is extended; the snapshots that change are read before they are accepted
- [x] 2.5 `crates/core/src/fact.rs` and `crates/lang-ts/src/analyze.rs`: `ImportFlags::without`, and a file with a load named at run time marks none of its mocks as replacing; fixture `named-loads` and the scenario `A load named at run time leaves no mock replacing its module`

## 3. The walks

- [x] 3.1 `crates/core/src/query/walk.rs` and `crates/core/src/query/walk/names.rs`: both walks record the files they reached across a contract
- [x] 3.2 `crates/core/src/query/walk/names.rs`: a mock that replaces its module takes no name, is no load and is no caller of a file followed whole

## 4. The filter

- [x] 4.1 `crates/core/src/graph.rs`: `Graph::reaches_avoiding`
- [x] 4.2 `crates/core/src/query/replaced.rs` and `crates/core/src/query/impact.rs`: the modules a test replaces, read from its own imports, and the filter over the tests of the walk with no depth
- [x] 4.3 `crates/core/src/testing.rs`: builders for a replacing mock and for an import that asks for the actual module
- [x] 4.4 `crates/core/src/query/replaced.rs`: a module the test also names in a mock call that carries no replacing mark is not replaced; fixture `unsure-mocks` in `crates/lang-ts/tests/` for each callee, and the scenario `A test that mocks the module again is listed`

## 5. The tests

- [x] 5.1 One test per scenario of `Tests that replace a module`, in `crates/core/tests/queries/impact.rs` and `crates/core/tests/queries/names.rs`, and one for each of the two scenarios added to `Callers at load` and `Mocks by name`
- [x] 5.2 Two properties over generated trees, with plain, replacing, actual and bare mock edges. By files, a test is listed exactly when the files it runs, by the edges alone, include the changed one. By names, making every mock of a tree one that replaces its module adds no file and no test to the answer and lists exactly the tests of the walk that run the change that way
- [x] 5.3 The tests that touch a mock keep their meaning: `crates/core/tests/queries/names.rs` and `crates/gsttt/tests/cli/impact.rs`
- [x] 5.4 Unit tests of `mocks::replaces` for a parameter used in a nested closure, a default value, `arguments`, a shadowed parameter, a parenthesised factory and `satisfies`; a changed test is no different for the module it replaces

## 6. The wording

- [x] 6.1 `docs/architecture.md`
