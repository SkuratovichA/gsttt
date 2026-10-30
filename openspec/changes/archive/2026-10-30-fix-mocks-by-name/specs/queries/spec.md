# Spec Delta

## MODIFIED Requirements

### Requirement: Callers at load
Code that runs as a file loads SHALL be a caller of what it refers to. A file whose load uses an asked name SHALL be listed as reached at load, a test file among the tests. A hop from a file reached at load SHALL reach at load every file that loads or mocks it, which a type-only import does not, and that file SHALL cross all its contracts. A mock counts because one made without a factory loads the module to mock it.

#### Scenario: A test that calls a name is a direct caller
- **WHEN** `a.test.ts` imports `foo` from `a.ts` and calls it in a callback of `test`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `a.test.ts` among the tests at distance 1, reached at load

#### Scenario: A file reached at load reaches the files that load it
- **WHEN** `config.ts` has `export const config = load()` with `load` imported from `a.ts` and exports the type `Config`, `app.ts` imports `config` from `config.ts`, `types.ts` imports only the type `Config` from it, and impact is asked about `load` in `a.ts` with depth 2
- **THEN** impact lists `config.ts` at distance 1 reached at load with the caller `config`, and `app.ts` at distance 2 reached at load
- **AND** it does not list `types.ts`

#### Scenario: A file reached at load reaches the files that mock it
- **WHEN** `config.ts` has `export const config = load()` with `load` imported from `a.ts`, `config.test.ts` has `vi.mock('./config')` and imports nothing from `config.ts`, and impact is asked about `load` in `a.ts` with depth 2
- **THEN** impact lists `config.test.ts` among the tests at distance 2, reached at load

## ADDED Requirements

### Requirement: Mocks by name
Asked about names, `impact` SHALL take no name through a mock, because a test that mocks a module runs what replaces it and not the code of the module. A mock of a file followed whole SHALL be a caller where it is written, as an import for side effects is.

#### Scenario: A mock takes no name
- **WHEN** `a.ts` declares `foo`, `a.test.ts` has `vi.mock('./a', () => ({ foo: vi.fn() }))` at its top level and imports nothing from `a.ts`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists no file and no test

#### Scenario: A mock of a file followed whole is a caller
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `b.test.ts` has `vi.mock('./b')` at its top level and imports nothing from `b.ts`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.test.ts` among the tests at distance 2, reached at load

### Requirement: Loads that name what they take
Asked about names, a dynamic import or a `require` whose connector records the names taken from it SHALL take those names and no others, where it is written. One that takes the module itself, as a namespace or with no names recorded, SHALL take every name.

#### Scenario: A load takes the names read from it
- **WHEN** `a.ts` declares `foo` and `bar`, `b.ts` has `const { bar } = await import('./a')` in `load`, `c.ts` has `(await import('./a')).foo()` at its top level, `d.ts` has `const lib = await import('./a')` in `start`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `c.ts` reached at load and `d.ts` with the caller `start`
- **AND** it does not list `b.ts`

### Requirement: Types outside declarations
Asked about names, an import type SHALL be a caller only in the declarations whose lines hold it. Outside every declaration it SHALL reach nothing, because a type does not run as the file loads.

#### Scenario: An import type outside every declaration reaches nothing
- **WHEN** `a.ts` declares `foo`, `a.test.ts` has `vi.mock('./a', async (importOriginal) => ({ ...(await importOriginal<typeof import('./a')>()) }))` at its top level and imports nothing else from `a.ts`, `types.ts` has `export type Api = typeof import('./a')`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `types.ts` with the caller `Api`
- **AND** it does not list `a.test.ts`
