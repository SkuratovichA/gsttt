# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the deltas of the `typescript` and `queries` specs

## 2. The facts

- [ ] 2.1 `ImportKind::Mock` after the other kinds, its label `mock`, the fact format at 4 and the snapshot layout at 7

## 3. TypeScript

- [ ] 3.1 The mock calls as mocks, and `vi.importActual` and `jest.requireActual` as dynamic imports, with the scenarios of `Mocked modules` and `Mocks made from or set for a module`
- [ ] 3.2 A member read by name from `await import('x')` or from `require('x')`, with the scenario of `Members read from a loaded module`

## 4. The walk by name

- [ ] 4.1 No name through a mock, a mock of a file followed whole, and the files that mock a file reached at load, with the scenarios of `Mocks by name` and `Callers at load`
- [ ] 4.2 The names a dynamic import or a `require` takes, with the scenario of `Loads that name what they take`
- [ ] 4.3 An import type outside every declaration, with the scenario of `Types outside declarations`
- [ ] 4.4 An end-to-end test of `gsttt impact --name` on a TypeScript tree with a mocked barrel, a partial mock and a member read from a loaded module, and the walk by name in `docs/architecture.md`
