# Proposal

## Why

Compared with dependency-cruiser on a large TypeScript monorepo, gsttt found 7,757 of the 7,860 import edges. The 103 it missed had two causes. 96 were types written `typeof import('./lib')`, mostly in test mocks such as `vi.importActual<typeof import('./lib')>('./lib')` and `importOriginal<typeof import('./editor')>()`, and 7 were Vite imports with a query, such as `import terms from '../docs/terms.md?raw'`. The mocked module itself, `vi.mock('./lib', …)`, was no edge either, so a change to a mocked module did not reach the tests that mock it.

## What Changes

- `typeof import('x')` and `import('x').T` in a type are static imports of `x` flagged type-only, at the line of the specifier, naming the first segment of the qualifier.
- The first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importActual`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock`, `jest.requireActual` and `jest.requireMock` is a dynamic import with no names when it is a literal. A computed one is neither an import nor an unread site.
- A query is dropped from a specifier before it is resolved, so `../docs/terms.md?raw` leads where `../docs/terms.md` leads. The fact keeps the specifier as written.
- The connector version goes from 1 to 2, so that facts cached by version 1 are read again.
- Tables found through aliases of `this`, typed roots and imports of model types (section 2 of the tasks).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `typescript`: three forms that the connector did not read as imports, and the queries that stopped a specifier from resolving.

## Impact

`crates/lang-ts/src/analyze.rs` gets a visitor for import types and reads the module calls of Vitest and Jest beside `require`, `crates/lang-ts/src/resolve.rs` drops the query before it resolves, and `crates/lang-ts/src/connector.rs` raises the version. No other crate changes. The index gains the edges it was missing, and every file of a TypeScript project is read again on the first run after the upgrade.
