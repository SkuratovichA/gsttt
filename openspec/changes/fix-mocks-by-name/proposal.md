# Proposal: mocks and loads by name

## Why

Asked about a name, `impact` lists test files that never run it. A test that mocks a module on the way, `vi.mock('./api', () => ({ fetchUser: vi.fn() }))`, is read as a dynamic import of it, and a dynamic import takes every name, so the test is listed as a caller of every name of `./api` and of every name a barrel on the way passes on. A partial mock, `vi.mock('./api', async (importOriginal) => ({ ...(await importOriginal<typeof import('./api')>()) }))`, also takes the namespace of the module as a type, and the type sits in code that runs at load. A test that reads one member of a module it loads, `(await import('./fixtures')).build()`, takes every name of that module as well, because the member it reads is not recorded.

On a repository whose tests mock their modules, an answer by name fills with such tests. An agent handed it greps the name again to throw them out, so the answer costs it turns instead of saving them.

## What Changes

- A new import kind, `mock`: the calls of Vitest and Jest that mock a module, stop mocking it, or make or set a mock for it. `vi.importActual` and `jest.requireActual` stay dynamic imports, because they load the module as it is. `dependents` labels a mock `mock`, and the walk over files follows it as it followed the dynamic import before.
- The TypeScript connector records a member read by name from `await import('x')` or from `require('x')` as the name that import takes.
- Asked about names, `impact` takes no name through a mock. A mock of a file followed whole is a caller where it is written, as an import for side effects is, and a file reached at load reaches the files that mock it, because a mock made without a factory loads the module to mock it.
- Asked about names, a dynamic import or a `require` that names what it takes takes those names and no others. The walk over files keeps taking every name through them, because what they hand back is the whole module.
- Asked about names, an import type outside every declaration reaches nothing, because a type does not run as the file loads.

## Impact

- `crates/core`: `ImportKind::Mock` and its label, the fact format at 4 and the snapshot layout at 7, the walk by name.
- `crates/lang-ts`: the mock calls, the members read from a loaded module, their fixtures and snapshots.
- Specs: `typescript` (Mocked modules, Mocks made from or set for a module, Members read from a loaded module) and `queries` (Callers at load, Mocks by name, Loads that name what they take, Types outside declarations).
