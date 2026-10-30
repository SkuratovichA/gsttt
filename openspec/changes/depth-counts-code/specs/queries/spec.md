# Spec Delta

## ADDED Requirements

### Requirement: A hop across a contract is no hop through code
The depth of `impact` SHALL bound the hops through code: imports, uses of a name, loads and mocks. A hop across a contract SHALL count toward the distance of a file and not toward the depth, so a file reached across a contract is followed through code as far as any other, in the walk by name and in the walk over files. The first line of an impact answer SHALL say `within N hop(s) through code`.

#### Scenario: A client of a route a caller serves is reached within the default depth
- **WHEN** `db.ts` declares `rows`, `api.ts` imports it and uses it in `listUsers`, which serves `http GET /users`, `users.ts` calls `http GET /users` in `fetchUsers`, `page.ts` imports `fetchUsers` and uses it in `Page`, and impact is asked about `rows` in `db.ts` without a depth
- **THEN** impact lists `api.ts` at distance 1 with the caller `listUsers` and `users.ts` at distance 2 with the reason `calls http GET /users` and the caller `fetchUsers`
- **AND** it does not list `page.ts`, and has a note that the walk stopped with callers left to follow

#### Scenario: A file reached across a contract is followed through code
- **WHEN** a controller serves `http GET /bookings/{}`, `api.ts` calls it, `page.tsx` imports `api.ts`, `app.tsx` imports `page.tsx`, and the controller changes with depth 1
- **THEN** impact lists `api.ts` at distance 1 and `page.tsx` at distance 2
- **AND** it does not list `app.tsx`

### Requirement: Tests left out of an impact answer
`impact` SHALL leave the tests out of its answer when asked to: it walks as it does otherwise, lists no tests, and says how many test files it left out, as `N test files left out` in the text and as a count beside the empty list of tests in the JSON. The coverage line SHALL still cover every file the walk reached, and an answer whose walk reached only tests SHALL NOT say that nothing is affected.

#### Scenario: The tests are counted and not listed
- **WHEN** `a.ts` changes and is imported by `b.ts` and by the test files `a.test.ts` and `b.test.ts`, and impact is asked to leave the tests out
- **THEN** impact lists `b.ts` and lists no tests
- **AND** it says it left out 2 test files

#### Scenario: Only tests reached is not nothing
- **WHEN** `a.ts` changes and is imported only by the test file `a.test.ts`, and impact is asked to leave the tests out
- **THEN** impact lists no file and says it left out 1 test file
- **AND** it does not say that nothing is affected

## MODIFIED Requirements

### Requirement: Impact through imports
`impact` SHALL return every file that reaches a changed file through import edges followed backwards to a fixpoint, with its distance in hops and its first reason; where the edges name what the importer takes, the change SHALL go only as far as the names allow, per the requirements on names below. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. A depth limit SHALL stop the walk at that many hops through code. The changed files SHALL NOT be listed as affected.

#### Scenario: A change reaches importers of importers
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, and `a.ts` changes
- **THEN** impact lists `b.ts` at distance 1 with the reason `imports a.ts` and `c.ts` at distance 2 with the reason `imports b.ts`

#### Scenario: A mod declaration carries no impact
- **WHEN** `lib.rs` declares `mod store;` and imports nothing else from `store.rs`, and `store.rs` changes
- **THEN** impact does not list `lib.rs`

#### Scenario: Depth limits the walk
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, and `a.ts` changes with depth 1
- **THEN** impact lists `b.ts` and not `c.ts`

#### Scenario: Import cycles terminate
- **WHEN** `a.ts` and `b.ts` import each other and `a.ts` changes
- **THEN** impact lists `b.ts` once and does not list `a.ts`

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
- **THEN** the text starts with `impact of foo in a.ts, within 1 hop through code` and lists `b.ts  distance 1  imports foo from a.ts; in run (line 3), stop (line 7)`
- **AND** the JSON entry of `b.ts` has the callers `run` at line 3 and `stop` at line 7

### Requirement: Depth by name
Asked about names, `impact` SHALL take one hop through code unless the request gives a depth. Each further hop SHALL go from the declarations reached to the declarations that refer to them, by the rules of the first. When the depth stops the walk with callers left to follow, the answer SHALL say so, and that asking about a caller or with a larger depth goes on.

#### Scenario: A larger depth follows the callers of callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` imports `run` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 with the caller `run` and `c.ts` at distance 2 with the reason `imports run from b.ts` and the caller `main`

#### Scenario: The answer says when the walk stopped at the callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` uses `run` in `main`, and impact is asked about `foo` in `a.ts` without a depth
- **THEN** the answer lists `b.ts` and not `c.ts`, and has a note that the walk stopped with callers left to follow

#### Scenario: A walk that runs out says nothing about stopping
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, nothing refers to `run`, and impact is asked about `foo` in `a.ts`
- **THEN** the answer lists `b.ts` and has no note about stopping
