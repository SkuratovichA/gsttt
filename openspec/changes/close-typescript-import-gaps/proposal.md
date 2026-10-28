# Proposal

## Why

Compared with dependency-cruiser on a large TypeScript monorepo, gsttt found 7,757 of the 7,860 import edges. The 103 it missed had two causes. 96 were types written `typeof import('./lib')`, mostly in test mocks such as `vi.importActual<typeof import('./lib')>('./lib')` and `importOriginal<typeof import('./editor')>()`, and 7 were Vite imports with a query, such as `import terms from '../docs/terms.md?raw'`. The mocked module itself, `vi.mock('./lib', …)`, was no edge either, so a change to a mocked module did not reach the tests that mock it.

On the same monorepo, `contract 'table Tenant'` found 136 of the 160 files that use the table. The files it missed import only the type of the model (21), call the client through `const self = this`, or call it through a parameter typed `Pick<Prisma.TransactionClient, 'tenant' | 'tenantBranch'>`. A mock such as `mockPrisma.tenant.findFirst()` is a miss on purpose, because a mock is no client.
## What Changes

- `typeof import('x')` and `import('x').T` in a type are static imports of `x` flagged type-only, at the line of the specifier, naming the first segment of the qualifier, or `*` when there is none, as a namespace import does.
- The first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importActual`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock`, `jest.requireActual`, `jest.requireMock`, `jest.setMock`, `jest.createMockFromModule` and `jest.genMockFromModule` is a dynamic import with no names when it is a literal. A computed one is neither an import nor an unread site.
- A query is dropped from a specifier before it is resolved, so `../docs/terms.md?raw` leads where `../docs/terms.md` leads. The fact keeps the specifier as written.
- The connector version goes from 1 to 2, so that facts cached by version 1 are read again.
- A chain rule reads a chain that starts at a name bound by `const self = this` or `let self = this` as a chain from `this`, so `self.prisma.tenant.findUnique()` calls `table tenant`.
- A chain rule has `root_types`. A parameter, a variable or a class property declared with one of them, alone, in a union, as the first type argument of `Pick`, `Omit`, `Partial`, `Required`, `Readonly` or `NonNullable`, or through a `type` alias of the same file, starts a chain, and a constructor parameter property starts one as `this.<name>`. The `prisma` pack lists `PrismaClient`, `PrismaService` and `Prisma.TransactionClient`.
- A pack has a sixth rule table, `[[imports]]`, with `from`, `kind`, `role`, `except` and `case`: each name a file imports from one of the packages in `from`, or re-exports from it by name, names a contract with the exported name as key at the line of the import. The `prisma` pack uses it for `@prisma/client`, except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`. A namespace import, a default import and a side-effect import name nothing. An enum imported from the client names a table that no model declares, because the import does not say which names are models.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `typescript`: three forms that the connector did not read as imports, the queries that stopped a specifier from resolving, and three ways a table was used that it did not see.
- `config`: the pack schema describes six kinds of rules, not five.

## Impact

`crates/lang-ts/src/analyze.rs` gets a visitor for import types and reads the module calls of Vitest and Jest beside `require`, `crates/lang-ts/src/resolve.rs` drops the query before it resolves, and `crates/lang-ts/src/connector.rs` raises the version. `crates/core/src/rules.rs` gets `root_types` on `ChainRule` and the rule kind `ImportRule` on `Pack`, `crates/lang-ts/src/analyze/roots.rs` finds the aliases of `this` and the typed names before the walk, `crates/lang-ts/packs/prisma.toml` uses both, and `crates/config` pins the schema that results. The Rust and Python connectors do not read the new fields. A pack that lists neither `root_types` nor `imports` serialises as before, so the cached facts of the Rust and Python connectors keep their keys. The index gains the edges and the tables it was missing, and every file of a TypeScript project is read again on the first run after the upgrade.
