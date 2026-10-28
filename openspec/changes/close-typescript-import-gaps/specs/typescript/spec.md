# Spec Delta

## ADDED Requirements

### Requirement: Imports in types
The connector SHALL read `typeof import('x')` and `import('x').T` in a type as a static import of `x` flagged type-only, at the line of the specifier. `typeof import('x')` SHALL name nothing, and `import('x').A.B` and `typeof import('x').A` SHALL name `A`, the first name of the qualifier.

#### Scenario: A type query of a module is a type-only import
- **WHEN** `src/types.ts` has `export type Lib = typeof import('./lib')` on line 1 and `export type Helper = typeof import('./lib').helper` on line 2, and `src/lib.ts` exists
- **THEN** `src/types.ts` has two static imports of `./lib`, both flagged type-only, the first at line 1 naming nothing and the second at line 2 naming `helper`
- **AND** both resolve to `src/lib.ts`

#### Scenario: An import type names the first segment of its qualifier
- **WHEN** `src/types.ts` has `export type Editor = import('./lib').Editor` on line 1 and `export type Inner = import('./lib').Outer.Inner` on line 2
- **THEN** the file has static imports of `./lib` flagged type-only, naming `Editor` at line 1 and `Outer` at line 2

#### Scenario: A nested import type is read at the line of its specifier
- **WHEN** `src/types.ts` has `typeof import(` on line 1 and `'./lib'` on line 2, `import('./lib').Box<typeof import('./other')>` on line 4 and `Promise<typeof import('./lib')>` on line 5
- **THEN** the file has type-only static imports of `./lib` at lines 2 and 5, and at line 4 one of `./lib` naming `Box` and one of `./other`

### Requirement: Mocked modules
The connector SHALL read the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importActual`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock`, `jest.requireActual` and `jest.requireMock` as a dynamic import with no names when it is a string or a template with no expression. A computed first argument SHALL be neither an import nor an unread site.

#### Scenario: A mocked or actual module is a dynamic import
- **WHEN** `src/runners.test.ts` has `'./lib'` as the first argument of each of the twelve calls above, one per line, and `` vi.mock(`./lib`) `` on line 13
- **THEN** each of the thirteen calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`

#### Scenario: A module named at run time is no import
- **WHEN** `src/computed.test.ts` has `vi.mock(path)`, `` jest.mock(`./${name}`) ``, `vi.importActual('./' + name)` and `await import(name)`
- **THEN** the file has no imports
- **AND** its only unread site is the one of `await import(name)`, with the reason computed at run time

#### Scenario: A mock call on another receiver is no import
- **WHEN** `src/others.test.ts` has `server.mock('./lib')`, `vi.fn('./lib')` and `mocker.importActual('./lib')`, and `src/lib.ts` exists
- **THEN** the file has no imports

#### Scenario: A mock that asks for the actual module reaches it
- **WHEN** `src/lib.test.ts` has `vi.mock('./lib', async () => ({ ...(await vi.importActual<typeof import('./lib')>('./lib')) }))` over lines 1 to 3 and `vi.mock('./editor', async (importOriginal) => ({ ...(await importOriginal<typeof import('./editor')>()) }))` over lines 5 to 7
- **THEN** the file imports `./lib` dynamically at lines 1 and 2 and as a type at line 2, and `./editor` dynamically at line 5 and as a type at line 6
- **AND** the imports resolve to `src/lib.ts` and `src/editor.ts`, so a change to either module reaches the test

### Requirement: Query suffixes
A specifier with a query, such as `./terms.md?raw`, `./logo.svg?url` or `ui-kit/theme.css?inline`, SHALL resolve as the same specifier without the query does, and the import SHALL keep the specifier as written.

#### Scenario: A query does not change where an import leads
- **WHEN** `src/page.ts` imports `../docs/terms.md?raw`, `./worker?worker`, `./logo.svg?url`, `ui-kit/theme.css?inline`, `../generated/schema.json?url` and `./absent.md?raw`, and each of them again without its query, where `docs/terms.md`, `src/worker.ts` and `src/logo.svg` exist, `ui-kit` is declared in `package.json`, `generated/schema.json` exists on disk but an ignore file keeps it out of the file list, and `absent.md` does not exist
- **THEN** each import with a query leads where the same specifier without the query leads
- **AND** the first three resolve to `docs/terms.md`, `src/worker.ts` and `src/logo.svg`, the fourth to the package `ui-kit`, the fifth is unresolved as ignored and the sixth as not found
- **AND** the facts keep the specifiers with their queries
