# Spec Delta

## REMOVED Requirements

### Requirement: Mocked modules
**Reason**: A mock is an import of its own kind now, so the calls this requirement read as dynamic imports are split between `Mocks of a module` and `Actual modules`.
**Migration**: None.

### Requirement: Mocks made from or set for a module
**Reason**: These calls are mocks now, read under `Mocks made or set for a module`.
**Migration**: None.

## ADDED Requirements

### Requirement: Mocks of a module
The connector SHALL read the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock` and `jest.requireMock` as a mock of the module with no names when it is a string or a template with no expression. A computed first argument SHALL be neither an import nor an unread site.

#### Scenario: A mocked module is a mock
- **WHEN** `src/runners.test.ts` has `'./lib'` as the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock` and `jest.requireMock`, one per line, and `` vi.mock(`./lib`) `` on line 11
- **THEN** each of the eleven calls is a mock of `./lib` at its line, with no names, resolving to `src/lib.ts`

#### Scenario: A module named at run time is no import
- **WHEN** `src/computed.test.ts` has `vi.mock(path)`, `` jest.mock(`./${name}`) ``, `vi.importActual('./' + name)` and `await import(name)`
- **THEN** the file has no imports
- **AND** its only unread site is the one of `await import(name)`, with the reason computed at run time

#### Scenario: A mock call on another receiver is no import
- **WHEN** `src/others.test.ts` has `server.mock('./lib')`, `vi.fn('./lib')` and `mocker.importActual('./lib')`, and `src/lib.ts` exists
- **THEN** the file has no imports

#### Scenario: A mock that asks for the actual module reaches it
- **WHEN** `src/lib.test.ts` has `vi.mock('./lib', async () => ({ ...(await vi.importActual<typeof import('./lib')>('./lib')) }))` over lines 1 to 3 and `vi.mock('./editor', async (importOriginal) => ({ ...(await importOriginal<typeof import('./editor')>()) }))` over lines 5 to 7
- **THEN** the file mocks `./lib` at line 1 and `./editor` at line 5, imports `./lib` dynamically and as a type at line 2, and imports `./editor` as a type at line 6
- **AND** the imports resolve to `src/lib.ts` and `src/editor.ts`, so a change to either module reaches the test

### Requirement: Actual modules
The connector SHALL read the first argument of `vi.importActual` and `jest.requireActual` as a dynamic import with no names when it is a string or a template with no expression, because both load the module as it is. A computed first argument SHALL be neither an import nor an unread site.

#### Scenario: An actual module is a dynamic import
- **WHEN** `src/actual.test.ts` has `vi.importActual('./lib')` on line 1 and `jest.requireActual('./lib')` on line 2, and `src/lib.ts` exists
- **THEN** each of the two calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`

### Requirement: Mocks made or set for a module
The connector SHALL read the first argument of `jest.setMock`, `jest.createMockFromModule`, `jest.genMockFromModule`, `jest.unstable_mockModule` and `jest.deepUnmock` as a mock of the module with no names when it is a string or a template with no expression, and a computed one SHALL be neither an import nor an unread site.

#### Scenario: A mock made from a module or set for it is a mock
- **WHEN** `src/made.test.ts` has `jest.setMock('./lib', {})`, `jest.createMockFromModule('./lib')`, `jest.genMockFromModule('./lib')`, `jest.unstable_mockModule('./lib', () => ({}))` and `jest.deepUnmock('./lib')` on lines 1 to 5, and `src/lib.ts` exists
- **THEN** each of the five calls is a mock of `./lib` at its line, with no names, resolving to `src/lib.ts`

### Requirement: Members read from a loaded module
A member read by name from `await import('x')` or from `require('x')` SHALL be the name that import takes. `import('x')` without `await` hands back a promise, so a member read from it SHALL name nothing.

#### Scenario: A member read from a loaded module is the name it takes
- **WHEN** `src/use.ts` has `(await import('./lib')).helper()` on line 1, `require('./lib').other` on line 2 and `import('./lib').then(start)` on line 3, and `src/lib.ts` exists
- **THEN** the file has a dynamic import of `./lib` naming `helper` at line 1, a `require` of it naming `other` at line 2, and a dynamic import of it with no names at line 3
