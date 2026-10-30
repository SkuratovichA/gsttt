# Spec Delta

## ADDED Requirements

### Requirement: Impact by name
Asked about names declared in a file, `impact` SHALL list their direct callers: the declarations that refer to them, in the file itself, which is then listed too, and in the files that import them, through re-exports and through imports exported again, which add no hop. Each entry SHALL name its callers with the lines they start on, in byte order and cut like the names of an edge. A name the file does not declare SHALL be named in a note and reach nothing.

#### Scenario: Only the direct callers are listed
- **WHEN** `a.ts` declares `foo` and `bar`, `b.ts` imports both and uses `foo` in `run` and `bar` in `stop`, `c.ts` imports `run` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `b.ts` at distance 1 with the reason `imports foo from a.ts` and the caller `run`
- **AND** it does not list `stop` or `c.ts`

#### Scenario: Callers in the asked file are listed
- **WHEN** `a.ts` declares `foo`, `helper`, which uses `foo`, and `other`, which does not, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `a.ts` at distance 1 with the reason `declares foo` and the caller `helper`, and not `other`

#### Scenario: A barrel adds no hop
- **WHEN** `index.ts` has `export { foo as bar } from './a'`, `page.ts` imports `bar` from `index.ts` and uses it in `render`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports foo as bar from a.ts`, and `page.ts` at distance 1 with the reason `imports bar from index.ts` and the caller `render`

#### Scenario: A name imported and exported again adds no hop
- **WHEN** `mid.ts` has `import { foo } from './a'` and `export { foo }`, `page.ts` imports `foo` from `mid.ts` and uses it in `render`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `mid.ts` at distance 1 with the reason `imports foo from a.ts` and no callers, and `page.ts` at distance 1 with the reason `imports foo from mid.ts` and the caller `render`

#### Scenario: A name the file does not declare reaches nothing
- **WHEN** `a.ts` declares `foo`, `b.ts` uses it in `run`, and impact is asked about `fooo` in `a.ts`
- **THEN** the answer lists no file and has a note that `a.ts` declares no `fooo`

#### Scenario: The answer names the callers and their lines
- **WHEN** `b.ts` declares `run` at line 3 and `stop` at line 7, both using `foo` imported from `a.ts`, and impact is asked about `foo` in `a.ts`
- **THEN** the text starts with `impact of foo in a.ts, within 1 hop` and lists `b.ts  distance 1  imports foo from a.ts; in run (line 3), stop (line 7)`
- **AND** the JSON entry of `b.ts` has the callers `run` at line 3 and `stop` at line 7

### Requirement: Members of a class
A member of a class SHALL be a declaration of its own, `Class.member`, so that a name used in one method affects that method and not its class. Asked about a member, `impact` SHALL list the members that use it through `this` and, because a call through a value names no class, the declarations that refer to the class, with a note that says so, one for all such members of the answer.

#### Scenario: A name used in a method affects that method
- **WHEN** `runner.ts` declares the class `Runner` with the method `start`, which calls `foo` imported from `a.ts`, and the method `stop`, `app.ts` imports `Runner` and uses it in `main`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.start`
- **AND** it does not list `Runner`, `Runner.stop` or `app.ts`

#### Scenario: Callers of a member are found through this and through its class
- **WHEN** `Runner.stop` calls `this.start()`, `app.ts` imports `Runner` and uses it in `main`, and impact is asked about `Runner.start` in `runner.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.stop` and `app.ts` with the caller `main`
- **AND** a note says that callers of `Runner.start` through a value are found through the declarations that refer to `Runner`

#### Scenario: Members of several classes are one note
- **WHEN** `runner.ts` declares the class `Runner` with the methods `start` and `stop` and the class `ModuleRunner` with the method `import`, `app.ts` imports `Runner` and uses it in `main`, and impact is asked about `Runner.start`, `Runner.stop` and `ModuleRunner.import` in `runner.ts`
- **THEN** impact lists only `app.ts`
- **AND** the answer has one note, that callers of `ModuleRunner.import`, `Runner.start` and `Runner.stop` through a value are found through the declarations that refer to their classes

### Requirement: Callers at load
Code that runs as a file loads SHALL be a caller of what it refers to. A file whose load uses an asked name SHALL be listed as reached at load, a test file among the tests. A hop from a file reached at load SHALL reach at load every file that loads it, which a type-only import does not, and that file SHALL cross all its contracts.

#### Scenario: A test that calls a name is a direct caller
- **WHEN** `a.test.ts` imports `foo` from `a.ts` and calls it in a callback of `test`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `a.test.ts` among the tests at distance 1, reached at load

#### Scenario: A file reached at load reaches the files that load it
- **WHEN** `config.ts` has `export const config = load()` with `load` imported from `a.ts` and exports the type `Config`, `app.ts` imports `config` from `config.ts`, `types.ts` imports only the type `Config` from it, and impact is asked about `load` in `a.ts` with depth 2
- **THEN** impact lists `config.ts` at distance 1 reached at load with the caller `config`, and `app.ts` at distance 2 reached at load
- **AND** it does not list `types.ts`

### Requirement: Depth by name
Asked about names, `impact` SHALL take one hop unless the request gives a depth. Each further hop SHALL go from the declarations reached to the declarations that refer to them, by the rules of the first. A file SHALL be listed once, at the distance the walk first reached it, and a caller or a load of it that the walk reached farther SHALL say its own distance. When the depth stops the walk with callers left to follow, the answer SHALL say so, and that asking about a caller or with a larger depth goes on.

#### Scenario: A larger depth follows the callers of callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` imports `run` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 with the caller `run` and `c.ts` at distance 2 with the reason `imports run from b.ts` and the caller `main`

#### Scenario: A caller reached farther than its file says its distance
- **WHEN** `b.ts` uses `foo` of `a.ts` in `inner`, which starts at line 1, `outer`, which starts at line 5, uses `inner`, the load of `b.ts` uses `inner`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts  distance 1  imports foo from a.ts; in inner (line 1), outer (line 5, distance 2); at load (distance 2)`
- **AND** the JSON entry of `b.ts` has the callers `inner` at distance 1 and `outer` at distance 2, and its load at distance 2
- **AND** asked with depth 1, impact lists `b.ts  distance 1  imports foo from a.ts; in inner (line 1)`

#### Scenario: The answer says when the walk stopped at the callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` uses `run` in `main`, and impact is asked about `foo` in `a.ts` without a depth
- **THEN** the answer lists `b.ts` and not `c.ts`, and has a note that the walk stopped with callers left to follow

#### Scenario: A walk that runs out says nothing about stopping
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, nothing refers to `run`, and impact is asked about `foo` in `a.ts`
- **THEN** the answer lists `b.ts` and has no note about stopping

### Requirement: Names in files without declarations
A file whose connector records no declarations SHALL be followed whole: asked about a name in such a file, `impact` SHALL walk from it as it does without names and say so in a note, and an importer without declarations, like an edge into a file whose exports are unknown, SHALL be followed whole from there. Python and Rust record no declarations yet.

#### Scenario: A Python file asked about by name is followed whole
- **WHEN** `user.py` imports `parse` from `lib.py`, `app.py` imports `user.py`, and impact is asked about `parse` in `lib.py`
- **THEN** impact lists `user.py` at distance 1 and not `app.py`
- **AND** a note says that `lib.py` is followed whole because its connector records no declarations

#### Scenario: An importer without declarations is followed whole
- **WHEN** `b.ts` imports `foo` from `a.ts` and was read past syntax errors, `c.ts` imports `bar` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 without callers and `c.ts` at distance 2 with the caller `main`

### Requirement: Contracts of a declaration
A contract site SHALL belong to the narrowest declaration whose lines hold it, a member before its class, and to the load of its file when none does. A declaration reached by name SHALL cross the contracts of its own sites, as a file affected in all its names crosses all of its, and no other contract of its file. Across a contract the walk SHALL reach the declaration each site of the other side belongs to, or the load of its file.

#### Scenario: A declaration crosses only its own contracts
- **WHEN** `api.ts` declares `listUsers`, which serves `http GET /users`, and `health`, which serves `http GET /health`, `users.ts` calls `http GET /users` in `fetchUsers`, `ping.ts` calls `http GET /health`, and impact is asked about `listUsers` in `api.ts`
- **THEN** impact lists `users.ts` at distance 1 with the reason `calls http GET /users` and the caller `fetchUsers`
- **AND** it does not list `ping.ts`
