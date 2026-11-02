# Spec Delta

## MODIFIED Requirements

### Requirement: Actual modules
The connector SHALL read the first argument of `vi.importActual` and `jest.requireActual` as a dynamic import when it is a string or a template with no expression, because both load the module as it is, and SHALL mark the import as asking for the actual module. Used as it stands, such an import SHALL name nothing. A computed first argument SHALL be neither an import nor an unread site, also when a member is read from it.

#### Scenario: An actual module is a dynamic import
- **WHEN** `src/actual.test.ts` has `vi.importActual('./lib')` on line 1 and `jest.requireActual('./lib')` on line 2, and `src/lib.ts` exists
- **THEN** each of the two calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`
- **AND** each import asks for the actual module

## ADDED Requirements

### Requirement: Mocks that replace a module
A `vi.mock` or `jest.mock` call that is a statement of its own at the top level of a file SHALL replace its module when its second argument is an arrow function or a function expression that does not use its first parameter, and the connector SHALL mark its mock as replacing. A call with no factory, a factory given by name or as an object, a factory that uses its first parameter or binds it in a pattern, a nested call, and any other mock call SHALL NOT replace it.

#### Scenario: A factory that does not use its parameter replaces the module
- **WHEN** `src/replacing.test.ts` has `vi.mock('./lib', () => ({ lib: 2 }))` on line 1, `jest.mock('./lib', function () { return { lib: 3 } })` on line 2 and `vi.mock('./lib', async (importOriginal) => ({ lib: 4 }))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that replaces it, resolving to `src/lib.ts`

#### Scenario: A factory that uses its parameter does not replace the module
- **WHEN** `src/partial.test.ts` has `vi.mock('./lib', async (importOriginal) => ({ ...(await importOriginal()) }))` on line 1, `jest.mock('./lib', async (original) => { const lib = await original(); return lib })` on line 2 and `vi.mock('./lib', async ({ lib }) => ({ lib }))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A mock without a factory does not replace the module
- **WHEN** `src/automock.test.ts` has `vi.mock('./lib')` on line 1, `vi.mock('./lib', factory)` on line 2 and `jest.mock('./lib', { spy: true })` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A nested mock call does not replace the module
- **WHEN** `src/nested.test.ts` has `vi.mock('./lib', () => ({}))` in the body of a `describe` callback on line 2 and `jest.mock('./lib', () => ({}))` in the body of a `beforeEach` callback on line 5, and `src/lib.ts` exists
- **THEN** each of the two calls is a mock of `./lib` at its line that does not replace it

#### Scenario: A mock that is not hoisted does not replace the module
- **WHEN** `src/late.test.ts` has `vi.doMock('./lib', () => ({}))` on line 1, `jest.doMock('./lib', () => ({}))` on line 2 and `jest.unstable_mockModule('./lib', () => ({}))` on line 3, and `src/lib.ts` exists
- **THEN** each of the three calls is a mock of `./lib` at its line that does not replace it

### Requirement: Loads named at run time
A file that takes a module by a name it does not spell out, with `import(name)`, `require(name)`, `vi.importActual(name)`, `jest.requireActual(name)` or another call of `vi` or `jest` that names a module, SHALL have no mock that replaces its module, because the load may be of that module as it is.

#### Scenario: A load named at run time leaves no mock replacing its module
- **WHEN** `src/import-name.test.ts` has `vi.mock('./lib', () => ({}))` on line 3 and `await import(name)` on line 5, `src/require-name.test.ts` has `jest.mock('./lib', () => ({}))` on line 3 and `require(name)` on line 5, `src/import-actual-name.test.ts` has `vi.mock('./lib', () => ({}))` on line 3 and `await vi.importActual(name)` on line 5, `src/require-actual-name.test.ts` has `jest.mock('./lib', () => ({}))` on line 3 and `jest.requireActual(name)` on line 5, and `src/lib.ts` exists
- **THEN** the mock of `./lib` on line 3 of each of the four files does not replace it
- **AND** the mock of `./lib` in a file with no such load, `src/replaced.test.ts`, does
