# Proposal: Leave a test out of an impact answer when it replaces the module the change reaches it through

## Why

A test that replaces a module with a factory, `vi.mock('./api', () => ({ fetchUser: vi.fn() }))`, runs none of the code of that module: the factory stands in for it, so a change to the module, or to anything only the module imports, cannot change what the test does. `impact` lists the test anyway, because the mock is an import edge like any other, and the test is listed with a reason it cannot have. Whoever runs the listed tests runs them for nothing, and whoever reads the list has to tell the tests that run the change from those that do not.

## What Changes

- The TypeScript reader marks a mock that replaces its module. A mock replaces its module when it is a `vi.mock` or `jest.mock` call that is a statement of its own at the top level of the file, and its second argument is an arrow function or a function expression that does not use its first parameter, the `importOriginal` of a partial mock. A mock without a factory, a factory given by name or as an object, a factory that uses its first parameter or binds it in a pattern, a call nested in another statement, and `vi.doMock`, `jest.doMock`, `jest.unstable_mockModule` and `jest.setMock` do not replace, because each of them loads the module, may load it, or is not hoisted over the imports of the file.
- The reader also marks the dynamic import that `vi.importActual` and `jest.requireActual` make, as asking for the actual module.
- `impact` takes a test out of its tests when it replaces a module and no way to the change is left. The modules a test replaces are the modules of its replacing mocks, less any module the test also takes in a way that may run its code, which is to ask for it as the actual module or to name it in any other mock call that does not replace it, a mock without a factory, an unmock, `importMock`, `createMockFromModule` and the like, because the index does not say what such a call loads. The modules are compared after resolution and read from each import of the test, not from the merged edge between two files. The test stays when imports that enter none of those modules lead from it to an entry: a changed file, or a file the walk reached across a contract. An import that asks for the actual module enters a replaced module all the same, whichever file on the way makes it, because that module then runs: a helper the test imports may take it with `vi.importActual` to build the factory. The crossing is the walk's own: the walk records the files it reached across a contract, in the walk over files and in the walk by name, and the filter reads them.
- The filter applies to the tests of the walk that has no depth, and to tests only. A test that stays keeps the distance, the reason, the callers and the load of the walk, `tests_left_out` counts the tests after the filter, and the footer still covers every file the walk reached.
- In the walk by name, a mock that replaces its module takes no name, is no load of the module and is no caller of a file followed whole, so a file that replaces a module is not listed as affected through it. The walk never reaches a changed file, so the filter is never asked about a changed test.
- A file that takes a module by a name it does not spell out, with `import(name)`, `require(name)`, `vi.importActual(name)`, `jest.requireActual(name)` or another `vi` or `jest` call that names a module, may load a module a mock replaces as it is, so the reader marks none of its mocks as replacing.
- Three new requirements, `Mocks that replace a module` and `Loads named at run time` for the reader and `Tests that replace a module` for `impact`, with the scenarios that prove them. `Actual modules` says that the import is marked, with its scenario carried and extended. `Callers at load` and `Mocks by name` say that a mock that replaces the module is no caller, each with every scenario it has carried word for word and one added.
- The version of the reader goes from 9 to 10, because the facts it produces for the same text change.

## Limitations

- A change to the types of a replaced module reaches the test only through the type checker, which `impact` does not model. After this change `impact` does not list the test through a type-only import of a replaced module either.
- A manual mock, a file in a `__mocks__` directory, still counts as the module it stands for: the reader does not know it replaces anything.
- `vi.mock(import('./a'), …)` is not read as a mock today, so it is not read as a replacing one.
- A factory that uses its first parameter, or binds it in a pattern, is read as a partial mock even if the parameter is only passed on, which keeps the test.
- Only the mocks written in the test itself count. A mock in a setup file that applies to every test does not remove a test.
- In the walk by name a file that replaces a module and also imports a type from it is still followed through that module, because the edge of the graph keeps a flag only when every import behind it carries it. It keeps a file in the answer and never drops one.
- The files `impact` lists as affected are not filtered: a file that reaches the change only through a module another file replaces is listed, and only the walk by name stops at a replacing mock.

- A file with a load named at run time loses the cut for all its mocks, also for the modules the load could not name. The test stays, which is the safe side.
- Only the calls that `vi` and `jest` offer for a module are known. A mock made through another helper, or a module loaded by a call that wraps `require`, is not seen.

## Impact

- Capabilities `typescript` and `queries`.
- `crates/core/src/fact.rs` (two flags of an import), `crates/core/src/graph.rs` (`Graph::reaches_avoiding`), `crates/core/src/query/walk.rs` and `crates/core/src/query/walk/names.rs` (the walks record the files they reached across a contract, and the three arms for a mock), `crates/core/src/query/impact.rs` and a new `crates/core/src/query/replaced.rs` (the filter), `crates/core/src/testing.rs` (builders for a replacing mock and an actual-module import), `crates/lang-ts/src/analyze.rs` with a new `crates/lang-ts/src/analyze/mocks.rs` (the factory), `docs/architecture.md`, the fixtures, tests and snapshots of the reader and the tests of the queries.
- `READER_VERSION` goes from 9 to 10, so indexes built by an earlier version read the TypeScript files again once.
- `FACT_FORMAT` stays at 13 and `LAYOUT` stays at 8: the filter is in the queries, which keep nothing between runs, and the two new flags take bits of the byte an import already holds, so no archived type changes shape.
- Cost: a test that replaces nothing pays one scan of its own imports. A test that replaces a module pays a walk forward over its imports, one bit per file, that stops at the first entry.
