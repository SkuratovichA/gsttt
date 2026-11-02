# Spec Delta

## MODIFIED Requirements
### Requirement: Impact through imports
`impact` SHALL return the files that reach a changed file through import edges followed backwards, with the distance in hops and the first reason each was reached, one hop through code unless the request gives a depth; where the edges name what the importer takes, the change SHALL go only as far as the names allow. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. The changed files SHALL NOT be listed as affected.

#### Scenario: A change reaches importers of importers
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, and `a.ts` changes with depth 2
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

#### Scenario: A walk takes one hop unless a depth is given
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, and `a.ts` changes
- **THEN** impact lists `b.ts` and not `c.ts`
- **AND** the text starts with `impact of a.ts, within 1 hop through code`
- **AND** it says the walk stopped at the depth with callers left to follow

#### Scenario: A large depth runs the walk to the end
- **WHEN** `d.ts` imports `c.ts`, `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, and `a.ts` changes with depth 4294967295
- **THEN** impact lists `b.ts`, `c.ts` and `d.ts`
- **AND** it does not say the walk stopped

### Requirement: Impact through names
Where the import edges name what the importer takes, `impact` SHALL carry a change name by name. A changed file and a file reached across a contract SHALL be affected in all their names, and an importer SHALL be affected in all its names when it imports by name an affected name of a file or a name the file does not export, and a file that re-exports such a name by name SHALL be affected in the names it exports it as.

#### Scenario: A changed barrel reaches every importer
- **WHEN** `index.ts` exports `own` and has `export * from './a'`, `form.ts` imports `fromA` from it, `page.ts` imports `own` from it, and `index.ts` changes
- **THEN** impact lists `form.ts` and `page.ts`

#### Scenario: A file that uses an affected name is affected in all its names
- **WHEN** `index.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` imports `fromA` from `a.ts` and exports `fromB`, `page.ts` imports `fromB` from `index.ts`, and `a.ts` changes with depth 3
- **THEN** impact lists `b.ts` at distance 1 with the reason `imports fromA from a.ts`, `index.ts` at distance 1, and `page.ts` at distance 3 with the reason `imports fromB from index.ts`

#### Scenario: A name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `old.ts` imports `gone` from `index.ts`, nothing exports `gone`, and `a.ts` changes with depth 2
- **THEN** impact lists `old.ts` with the reason `imports gone from index.ts`

#### Scenario: A re-export of a name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `outer.ts` exports `gone`, `kept` and `stable` and has `export { gone, kept } from './index'`, `page.ts` imports `gone` from `outer.ts`, `other.ts` imports `stable` from it, nothing else exports `gone`, and `a.ts` changes with depth 3
- **THEN** impact lists `outer.ts` at distance 2 with the reason `re-exports gone, kept from index.ts` and `page.ts` at distance 3 with the reason `imports gone from outer.ts`
- **AND** it does not list `other.ts`

### Requirement: Imports that take every name
An importer that imports a namespace, imports for side effects, dynamically, by `require`, or with names its connector does not record SHALL be affected in all its names as soon as any name of the file is affected, and so SHALL a file whose own exports are unknown.

#### Scenario: A namespace import takes every name
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `fromA`, `all.ts` imports `index.ts` as a namespace, `side.ts` imports it for its side effects, and `a.ts` changes with depth 2
- **THEN** impact lists `all.ts` and `side.ts`, each with the reason `imports index.ts`

#### Scenario: Unknown exports keep every importer
- **WHEN** `index.ts` has `export * from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes with depth 2
- **THEN** impact lists `index.ts` and `page.ts`

### Requirement: Names through re-exports
A named re-export SHALL affect the names it exports affected names as, and `export *` every affected name but `default`, where all the names of a file are those it exports itself and through its own `export *`. A file affected in some of its names only SHALL be listed with the reason that it re-exports them, each by the name the file it re-exports has for it, followed by `as` and the name it is exported as when that is another, so that a search of the file for what the reason names finds it.

#### Scenario: A barrel passes on only what it re-exports
- **WHEN** `index.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` exports `fromB`, `form.ts` imports `fromA` from `index.ts`, `page.ts` imports `fromB` from it, and `a.ts` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports fromA from a.ts` and `form.ts` at distance 2 with the reason `imports fromA from index.ts`
- **AND** it does not list `page.ts`

#### Scenario: A star does not forward default
- **WHEN** `index.ts` exports `default` and `own` and has `export * from './a'`, `a.ts` exports `default` and `fromA`, `form.ts` imports `fromA` from `index.ts`, `page.ts` imports `default` from it, and `a.ts` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports fromA from a.ts` and `form.ts` at distance 2
- **AND** it does not list `page.ts`, which takes the default export of `index.ts` itself

#### Scenario: A named re-export passes on its alias
- **WHEN** `index.ts` has `export { useFoo as useBar } from './foo'` and `export { other } from './other'`, `x.ts` imports `useBar` from `index.ts`, `y.ts` imports `other` from it, and `foo.ts` changes with depth 2
- **THEN** impact lists `index.ts` with the reason `re-exports useFoo as useBar from foo.ts` and `x.ts` with the reason `imports useBar from index.ts`
- **AND** it does not list `y.ts`

#### Scenario: Chained barrels pass names on
- **WHEN** `app.ts` imports `fromA` from `index.ts`, `other.ts` imports `fromB` from it, `index.ts` has `export * from './inner'`, `inner.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` exports `fromB`, and `a.ts` changes with depth 3
- **THEN** impact lists `inner.ts` at distance 1, `index.ts` at distance 2 and `app.ts` at distance 3
- **AND** it does not list `other.ts`

### Requirement: Star re-exports keep their importers
A file with `export *` SHALL be reached when the file it re-exports is affected, however few names that file still passes on, and none included, and it SHALL be listed with the reason that it re-exports the file. Its importers SHALL then be measured against the names it exports like those of any other barrel, so that a name nothing exports any more is affected. This SHALL hold on every hop of a chain of star re-exports.

#### Scenario: A file that exports nothing more still reaches the importers of its barrel
- **WHEN** `index.ts` exports nothing itself and has `export * from './only'`, `only.ts` exported `foo` and exports nothing now, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports only.ts` and `page.ts` at distance 2 with the reason `imports foo from index.ts`

#### Scenario: A file left with only a default export still reaches the importers of its barrel
- **WHEN** `index.ts` exports nothing itself and has `export * from './only'`, `only.ts` exports only `default`, which `export *` never forwards, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports only.ts` and `page.ts` at distance 2 with the reason `imports foo from index.ts`

#### Scenario: A star barrel behind a star barrel reaches the importers of both
- **WHEN** `mid.ts` has `export * from './only'`, `index.ts` has `export * from './mid'`, neither exports anything itself, `only.ts` exports nothing now, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes with depth 3
- **THEN** impact lists `mid.ts` at distance 1, `index.ts` at distance 2 with the reason `re-exports mid.ts`, and `page.ts` at distance 3 with the reason `imports foo from index.ts`

### Requirement: Tests that replace a module
A test that replaces a module with a factory mock runs none of its code, so `impact` SHALL list it only when imports lead from it to a changed file, or to a file the walk reached across a contract, entering no replaced module except by asking for the actual module. A module the test also takes by another mock call, or as the actual module, SHALL NOT count as replaced. A listed test SHALL keep the walk's distance, reason, callers and load, and one cut SHALL NOT count among the tests left out.

#### Scenario: A test that replaces the only way to the change is not listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b', () => ({}))` at its top level, and `a.ts` changes
- **THEN** impact lists `b.ts` and lists no test

#### Scenario: A test that has another way to the change is listed
- **WHEN** `b.ts` and `c.ts` import `a.ts`, `t.test.ts` imports `b.ts` and `c.ts` and has `vi.mock('./b', () => ({}))` at its top level, and `a.ts` changes with depth 2
- **THEN** impact lists `t.test.ts` among the tests at distance 2 with the reason `imports b.ts`

#### Scenario: A test that asks for the actual module is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b', async () => ({ ...(await vi.importActual('./b')) }))` at its top level, and `a.ts` changes with depth 2
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test whose helper asks for the replaced module as it is is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and `helper.ts` and has `vi.mock('./b', () => ({}))` at its top level, `helper.ts` takes `b.ts` through `vi.importActual('./b')`, and `a.ts` changes with depth 2
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test that mocks a module without a factory is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b')` at its top level, and `a.ts` changes with depth 2
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test that mocks the module again is listed
- **WHEN** `b.ts` imports `a.ts`, `t.test.ts` imports `b.ts` and has `vi.mock('./b', () => ({}))` and `vi.unmock('./b')` at its top level, and `a.ts` changes with depth 2
- **THEN** impact lists `t.test.ts` among the tests at distance 2

#### Scenario: A test that replaces the module on the way to a contract is not listed
- **WHEN** `server.ts` serves `GET /x`, `client.ts` calls it, `api.ts` imports `client.ts`, `t.test.ts` imports `api.ts` and has `vi.mock('./api', () => ({}))` at its top level, and `server.ts` changes with depth 2
- **THEN** impact lists `client.ts` and `api.ts` and lists no test

#### Scenario: A test that replaces the module on the way to a contract is not listed by name
- **WHEN** `server.ts` declares `handle` and serves `GET /x`, `client.ts` calls `GET /x` in `send`, `api.ts` uses `send` of `client.ts` in `post`, `t.test.ts` calls `post` of `api.ts` in a callback of `test` and has `vi.mock('./api', () => ({}))` at its top level, and impact is asked about `handle` in `server.ts` with depth 2
- **THEN** impact lists `client.ts` and `api.ts` and lists no test

### Requirement: Ties between equally near ways
When several ways reach a file at the same distance, `impact` SHALL give as its reason the way from the file with the smallest path, and `path` SHALL reach each file of its chain from the file with the smallest path one hop nearer. From one file an import SHALL come before a contract, contracts SHALL go in key order, and a file with several sites in one contract SHALL give the first by line of those the walk reaches.

#### Scenario: The reason comes from the file with the smallest path
- **WHEN** `w.ts` imports `a.ts` and `b.ts`, `z.ts` imports `a.ts`, `y.ts` imports `b.ts`, `x.ts` imports `y.ts` and `z.ts`, and `a.ts` and `b.ts` change with depth 2, asked in either order
- **THEN** impact gives `imports a.ts` as the reason for `w.ts` and `imports y.ts` as the reason for `x.ts`

#### Scenario: From one file an import comes before a contract
- **WHEN** `lib.ts` serves `http GET /x`, `x.ts` imports `lib.ts` and calls `http GET /x`, and `lib.ts` changes
- **THEN** impact gives `imports lib.ts` as the reason for `x.ts`

#### Scenario: A file's first site by line gives the reason
- **WHEN** `schema.yaml` declares `http GET /x`, `early-caller.ts` serves it at line 9 and calls it at line 3, and `schema.yaml` changes
- **THEN** impact gives `calls http GET /x` as the reason for `early-caller.ts`

#### Scenario: A chain comes from the file with the smallest path
- **WHEN** `start.ts` imports `a.ts` and `b.ts`, which both import `goal.ts`
- **THEN** the path from `start.ts` to `goal.ts` is `start.ts`, `a.ts`, `goal.ts`

### Requirement: Unknown exports mean the whole file
An edge into a file whose exports are unknown SHALL be followed at file level, whatever names it carries: its importer SHALL be affected in all its names, with the reason that it imports the file and no names, whether it imports by name, passes names on or re-exports everything. A file that re-exports everything from a file with unknown exports has unknown exports too. A connector that lists no exports SHALL therefore give the files, distances and reasons of a walk over files.

#### Scenario: A named import of a file with unknown exports is followed at file level
- **WHEN** `user.py` imports `parse` from `lib.py`, the exports of `lib.py` are unknown, and `lib.py` changes
- **THEN** impact lists `user.py` at distance 1 with the reason `imports lib.py`, which is what it lists when `user.py` imports `lib.py` without names

#### Scenario: A star re-export of a file with unknown exports is followed at file level
- **WHEN** `index.py` re-exports everything from `lib.py`, the exports of `lib.py` are unknown, `app.py` imports `parse` from `index.py`, and `lib.py` changes with depth 2
- **THEN** impact lists `index.py` at distance 1 with the reason `imports lib.py` and `app.py` at distance 2 with the reason `imports index.py`, which is what it lists when each of them imports its file without names

#### Scenario: A named re-export of a file with unknown exports is followed at file level
- **WHEN** `index.ts` exports `thing` with `export { thing } from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `imports legacy.js`, as it does when `index.ts` imports `legacy.js` without names, and `page.ts` at distance 2 with the reason `imports thing from index.ts`

#### Scenario: A barrel that re-exports a file with unknown exports has unknown exports too
- **WHEN** `index.ts` exports `own` and has `export * from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes with depth 2
- **THEN** impact lists `index.ts` at distance 1 with the reason `imports legacy.js` and `page.ts` at distance 2 with the reason `imports index.ts`

## ADDED Requirements

### Requirement: The depth bounds the tests
The depth of an `impact` answer SHALL bound the tests as it bounds the other files, for the walk by files and the walk by names alike. A test past the depth SHALL NOT be listed and SHALL be counted among the test files left out, from the same walk with no depth, each listed test keeping the distance, the reason, the callers and the load of that walk.

#### Scenario: A test past the depth of a walk by name is counted
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` imports `run` from `b.ts` and uses it in `main`, `c.test.ts` imports `main` from `c.ts` and calls it in a callback of `test`, and impact is asked about `foo` in `a.ts` without a depth
- **THEN** impact lists `b.ts` and not `c.ts`, and lists no tests
- **AND** it says it left out 1 test file
- **AND** the text starts with `impact of foo in a.ts, within 1 hop through code`

#### Scenario: A test past the depth of a walk by files is counted
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, `c.test.ts` imports `c.ts`, and `a.ts` changes with depth 1
- **THEN** impact lists `b.ts` and not `c.ts`, and lists no tests
- **AND** it says it left out 1 test file

#### Scenario: A depth that reaches a test lists it
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, `c.test.ts` imports `c.ts`, and `a.ts` changes with depth 3
- **THEN** impact lists `b.ts` and `c.ts`, and lists `c.test.ts` among the tests at distance 3 with the reason `imports c.ts`

#### Scenario: A test the depth reaches is listed beside one past it
- **WHEN** `a.test.ts` imports `a.ts`, `b.ts` imports `a.ts`, `b.test.ts` imports `b.ts`, and `a.ts` changes
- **THEN** impact lists `b.ts`, lists `a.test.ts` among the tests and not `b.test.ts`
- **AND** it says it left out 1 test file

#### Scenario: Tests left out past the depth are counted
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, `c.test.ts` imports `c.ts`, `a.ts` changes with depth 1, and impact is asked to leave the tests out
- **THEN** impact lists `b.ts` and lists no tests
- **AND** it says it left out 1 test file

## REMOVED Requirements

### Requirement: Tests past the depth
**Reason**: The depth bounded the affected files and not the tests, and now bounds both, so the requirement and its scenarios move to `The depth bounds the tests`.
**Migration**: A test past the depth is counted among the test files left out and not listed; ask with a larger depth to list it.
