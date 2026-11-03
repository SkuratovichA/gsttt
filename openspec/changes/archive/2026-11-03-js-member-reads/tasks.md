# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The TypeScript connector

- [x] 2.1 A require in a conditional, assigned to a local of the file, or read by a computed member binds
- [x] 2.2 `is_global_builtin` beside `is_builtin`, and the `receivers` pass that finds the names bound to a builtin
- [x] 2.3 A member read on such a name records no read
- [x] 2.4 `READER_VERSION` 34 to 35

## 3. The core walk

- [x] 3.1 A class taken by a require below the top level is used where the member is named or read
- [x] 3.2 The doc of `Symbols.reads`

## 4. The scenarios

- [x] 4.1 TypeScript: fixtures and tests for the three require forms
- [x] 4.2 TypeScript: a fixture and tests for the three scenarios of a read on a builtin
- [x] 4.3 Queries: a test for "A class taken by a require below the top level is used where the member is named"
