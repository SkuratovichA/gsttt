# Spec Delta

## MODIFIED Requirements

### Requirement: Callers at load
Code that runs as a file loads SHALL be a caller of what it refers to. A file whose load uses an asked name SHALL be listed as reached at load, a test file among the tests. A hop from a file reached at load SHALL reach at load every file that loads or mocks it, which a type-only import does not, and that file SHALL cross all its contracts. A mock counts because one made without a factory loads the module to mock it, and one that replaces the module does not.

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

#### Scenario: A file reached at load does not reach the files that replace it
- **WHEN** `config.ts` has `export const config = load()` with `load` imported from `a.ts`, `setup.ts` has `vi.mock('./config', () => ({ config: {} }))` at its top level and imports nothing from `config.ts`, and impact is asked about `load` in `a.ts` with depth 2
- **THEN** impact lists `config.ts` at distance 1 reached at load and does not list `setup.ts`

### Requirement: Mocks by name
Asked about names, `impact` SHALL take no name through a mock, because a test that mocks a module runs what replaces it and not the code of the module. A mock of a file followed whole SHALL be a caller where it is written, as an import for side effects is, unless it replaces the file.

#### Scenario: A mock takes no name
- **WHEN** `a.ts` declares `foo`, `a.test.ts` has `vi.mock('./a', () => ({ foo: vi.fn() }))` at its top level and imports nothing from `a.ts`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists no file and no test

#### Scenario: A mock of a file followed whole is a caller
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `b.test.ts` has `vi.mock('./b')` at its top level and imports nothing from `b.ts`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.test.ts` among the tests at distance 2, reached at load

#### Scenario: A mock that replaces a file followed whole is no caller
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `setup.ts` has `vi.mock('./b', () => ({}))` at its top level and imports nothing from `b.ts`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` and does not list `setup.ts`

## ADDED Requirements

### Requirement: Tests that replace a module
A test that replaces a module with a mock that has a factory runs none of its code, so `impact` SHALL list the test only when imports that enter no module it replaces lead from it to a changed file, or to a file the walk reached across a contract. A module the test also asks for as the actual module SHALL NOT count as replaced. A test that is listed SHALL keep the distance, the reason, the callers and the load of the walk, and one left out SHALL NOT be counted among the tests left out.

#### Scenario: A test that replaces the only way to the change is not listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b', () => ({}))` at its top level, and `a.ts` changes
- **THEN** impact lists `b.ts` and lists no test

#### Scenario: A test that has another way to the change is listed
- **WHEN** `b.ts` and `c.ts` import `a.ts`, `t.test.ts` imports `b.ts` and `c.ts` and has `vi.mock('./b', () => ({}))` at its top level, and `a.ts` changes
- **THEN** impact lists `t.test.ts` among the tests at distance 2 with the reason `imports b.ts`

#### Scenario: A test that asks for the actual module is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b', async () => ({ ...(await vi.importActual('./b')) }))` at its top level, and `a.ts` changes
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test that mocks a module without a factory is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b')` at its top level, and `a.ts` changes
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test that replaces the module on the way to a contract is not listed
- **WHEN** `server.ts` serves `GET /x`, `client.ts` calls it, `api.ts` imports `client.ts`, `t.test.ts` imports `api.ts` and has `vi.mock('./api', () => ({}))` at its top level, and `server.ts` changes
- **THEN** impact lists `client.ts` and `api.ts` and lists no test

#### Scenario: A test that replaces the module on the way to a contract is not listed by name
- **WHEN** `server.ts` declares `handle` and serves `GET /x`, `client.ts` calls `GET /x` in `send`, `api.ts` uses `send` of `client.ts` in `post`, `t.test.ts` calls `post` of `api.ts` in a callback of `test` and has `vi.mock('./api', () => ({}))` at its top level, and impact is asked about `handle` in `server.ts` without a depth
- **THEN** impact lists `client.ts` and `api.ts` and lists no test
