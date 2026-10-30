# queries Specification

## Purpose
Queries read an index and answer what a change affects and how files and contracts connect, as short text for people and agents and as JSON for scripts. Every answer says what it searched, how much of the tree was read and how fresh the index is.

## Requirements

### Requirement: Impact through imports
`impact` SHALL return every file that reaches a changed file through import edges followed backwards to a fixpoint, with its distance in hops and the first reason it was reached; where the edges name what the importer takes, the change SHALL go only as far as the names allow, per the requirements on names below. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. A depth limit SHALL stop the walk at that many hops. The changed files SHALL NOT be listed as affected.

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

### Requirement: Impact across contracts
From every changed and every affected file, `impact` SHALL cross the contracts the file has sites of: a `declares` site reaches every file that serves or calls the contract, a `serves` site reaches every file that calls it, and a `calls` site reaches no file. An HTTP contract SHALL be crossed to the sites of every key that meets it. The reason of a file reached across a contract SHALL be its own role and key.

#### Scenario: Changing a schema reaches servers and callers
- **WHEN** `openapi.yaml` declares `http GET /bookings/{}`, `bookings.controller.ts` serves it, `client.ts` calls it, and `openapi.yaml` changes
- **THEN** impact lists `bookings.controller.ts` with the reason `serves http GET /bookings/{}` and `client.ts` with the reason `calls http GET /bookings/{}`

#### Scenario: Changing a server reaches its callers only
- **WHEN** `openapi.yaml` declares `http GET /bookings/{}`, `bookings.controller.ts` serves it, `client.ts` calls it, and `bookings.controller.ts` changes
- **THEN** impact lists `client.ts` and does not list `openapi.yaml`

#### Scenario: Changing a caller reaches no server
- **WHEN** `client.ts` calls `http GET /bookings/{}`, nothing imports `client.ts`, and `client.ts` changes
- **THEN** impact is empty

#### Scenario: A call reaches a route through a parameter
- **WHEN** a controller serves `http GET /bookings/{}`, `app.ts` calls `http GET /bookings/me`, and the controller changes
- **THEN** impact lists `app.ts` with the reason `calls http GET /bookings/me`

#### Scenario: Contract and import hops chain
- **WHEN** a controller serves `http GET /bookings/{}`, `api.ts` calls it, `page.tsx` imports `api.ts`, and the controller changes
- **THEN** impact lists `api.ts` at distance 1 and `page.tsx` at distance 2

### Requirement: Impact output
`impact` SHALL order affected files by distance and then by path, SHALL list files tagged as tests in a group of their own, and SHALL report each input path that is not in the index: a path the caller reports as deleted together with the unresolved imports that may have pointed at it, and any other path with the words `not in the index`.

#### Scenario: Tests are listed apart
- **WHEN** `a.ts` changes and is imported by `b.ts` and by the test file `a.test.ts`
- **THEN** impact lists `b.ts` under affected files and `a.test.ts` under tests

#### Scenario: A deleted file is listed, not traced
- **WHEN** impact is asked about `src/old.ts`, which the caller reports as deleted, and `src/a.ts` has the unresolved import `./old` at line 4
- **THEN** the answer lists `src/old.ts` as deleted with `src/a.ts:4` importing `./old` as a possible miss

#### Scenario: An unknown path is named
- **WHEN** impact is asked about `notes.txt`, which no connector owns
- **THEN** the answer says that `notes.txt` is not in the index

### Requirement: Neighbours
`dependents` SHALL list the files that import a file and `dependencies` the files it imports, one level, each with the kinds of its edge and, where the importer names them, the names it takes from the file. `dependencies` SHALL also list the file's imports that resolved to packages, to builtins and to nothing, the last with their reasons.

#### Scenario: Dependencies show every outcome
- **WHEN** `src/a.ts` imports `./b`, `react`, `node:fs` and `./missing`
- **THEN** dependencies lists `src/b.ts` with its edge kind, the package `react`, the builtin `node:fs` and `./missing` with the reason `no such file`

#### Scenario: Dependents show edge kinds
- **WHEN** `b.ts` imports `a.ts` and `c.ts` re-exports everything from it
- **THEN** dependents of `a.ts` lists `b.ts` with a static import and `c.ts` with a re-export of everything

#### Scenario: Neighbours show the names taken
- **WHEN** `b.ts` imports `useFoo` and `useBar` from `a.ts`, `c.ts` re-exports `useFoo` from it as `useBaz`, and `d.ts` imports it for its side effects
- **THEN** dependents of `a.ts` lists `b.ts  static import  useBar, useFoo`, `c.ts  re-export  useFoo` and `d.ts  side-effect import`
- **AND** dependencies of `b.ts` lists `a.ts  static import  useBar, useFoo`

### Requirement: Dependencies of a file that was not read cleanly
`dependencies` of a file whose connector failed to parse it SHALL say that the file could not be parsed and so its imports are unknown, with the failure. For a file the connector read past syntax errors it SHALL say that imports may be missing. It SHALL NOT say of either that the file imports nothing.

#### Scenario: A file that failed to parse is not one that imports nothing
- **WHEN** `ts` owns `src/broken.ts`, its parse failed with a syntax error at line 3, and its dependencies are asked
- **THEN** the answer says that `src/broken.ts` could not be parsed, so its imports are unknown, names the syntax error, and does not say that the file imports nothing

#### Scenario: A file read past syntax errors says its imports may be missing
- **WHEN** `ts` read `src/rough.ts` past 2 syntax errors, the first at line 7, and found no imports in it
- **THEN** the dependencies of `src/rough.ts` say that it was read past 2 syntax errors, the first at line 7, so imports may be missing, and do not say that it imports nothing

### Requirement: Paths between files
`path` SHALL return a shortest chain from one file to another, where a step is an import, a hop from a file that calls a contract to a file that serves or declares it, or a hop from a file that serves a contract to a file that declares it. HTTP hops SHALL follow keys that meet. When there is no chain, the answer SHALL say so and how many files are reachable from the first file.

#### Scenario: A chain runs through a contract
- **WHEN** `page.tsx` imports `api.ts`, `api.ts` calls `http GET /bookings/{}`, and `bookings.controller.ts` serves it
- **THEN** the path from `page.tsx` to `bookings.controller.ts` is `page.tsx`, `api.ts` by import, `bookings.controller.ts` by `http GET /bookings/{}`

#### Scenario: No chain is reported with its reach
- **WHEN** a path is asked between two files with no chain between them
- **THEN** the answer says there is none and how many files the first one reaches

### Requirement: Ties between equally near ways
When several ways reach a file at the same distance, `impact` SHALL give as its reason the way from the file with the smallest path, and `path` SHALL reach each file of its chain from the file with the smallest path one hop nearer. From one file an import SHALL come before a contract, contracts SHALL go in key order, and a file with several sites in one contract SHALL give the first by line of those the walk reaches.

#### Scenario: The reason comes from the file with the smallest path
- **WHEN** `w.ts` imports `a.ts` and `b.ts`, `z.ts` imports `a.ts`, `y.ts` imports `b.ts`, `x.ts` imports `y.ts` and `z.ts`, and `a.ts` and `b.ts` change, asked in either order
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

### Requirement: Contract lookup
`contract` SHALL take a canonical key and list its sites grouped by role, and for an HTTP key also the sites of every key that meets it, each with its own key. Text that is not a canonical key SHALL be searched case-insensitively in every key, and the answer SHALL list the matching keys with the number of sites of each role. A blank query SHALL be an error that says what to ask for, not an answer.

#### Scenario: A route lists its callers
- **WHEN** `http GET /bookings/{}` is looked up, one file serves that key and another calls `http GET /bookings/me`
- **THEN** the answer lists the server under serves and the caller under calls with the key `http GET /bookings/me`

#### Scenario: Free text searches keys
- **WHEN** `booking` is looked up and the index has `table bookings`, `graphql Query.bookings` and `env API_URL`
- **THEN** the answer lists `table bookings` and `graphql Query.bookings` with their site counts and not `env API_URL`

#### Scenario: A blank query is an error
- **WHEN** `contract` is asked for empty text or only whitespace, and the index holds contracts
- **THEN** it returns an error that says to give a contract key or part of one
- **AND** it does not list the contracts of the index

### Requirement: HTTP matching
Two HTTP keys SHALL meet when their methods are equal or either is `*`, and their paths have the same number of segments, each pair equal or `{}` on either side, with or without one prefix from the index's `http_prefixes` removed from the start of one of the two paths. A prefix SHALL NOT be removed from both paths, and SHALL NOT be removed from a path that it is all of.

#### Scenario: A prefix is stripped before matching
- **WHEN** `http_prefixes` is `["/api"]` and `http GET /api/bookings/me` is compared with `http GET /bookings/{}`
- **THEN** they meet

#### Scenario: A prefix is removed from one path only
- **WHEN** `http_prefixes` is `["/api", "/v1"]` and `http GET /api/users` is compared with `http GET /v1/users`
- **THEN** they do not meet

#### Scenario: A prefix alone is no path
- **WHEN** `http_prefixes` is `["/api"]` and `http GET /api` is compared with `http GET /`
- **THEN** they do not meet

#### Scenario: Methods must agree
- **WHEN** `http POST /bookings` is compared with `http GET /bookings`, and `http * /bookings` with each of them
- **THEN** the first pair does not meet and `http * /bookings` meets both

#### Scenario: Segment counts must agree
- **WHEN** `http GET /bookings/{}` is compared with `http GET /bookings/{}/items`
- **THEN** they do not meet

#### Scenario: Meeting is symmetric
- **WHEN** any two HTTP keys and any prefixes are compared in both orders
- **THEN** both comparisons give the same result

### Requirement: Coverage answer
`coverage` SHALL begin with every problem of the index, one line each and never cut (the key `problems` in JSON), and SHALL list, per connector, its projects, its files by parse status (read cleanly, read past syntax errors, failed, skipped), the share of imports resolved, the unresolved imports and unread sites by reason with the first place of each, and what it provides with its blind spots, followed by the unclaimed files by language and the files skipped before any connector read them.

#### Scenario: Coverage names failures and blind spots
- **WHEN** an index has a TypeScript file that failed to parse, an unresolved import and a `fetch` with a computed URL
- **THEN** the coverage answer names the failed file with its failure, the import's reason with its first place and the unread site's reason with its first place

#### Scenario: A skipped file is owned and not read
- **WHEN** `ts` owns four files, of which one is over the size limit and one is not UTF-8, so that the engine lists both as skipped
- **THEN** the coverage answer gives for `ts` 2 files read cleanly, 0 read past syntax errors, 0 failed and 2 skipped
- **AND** its coverage line says `ts read 2 of 4 files`

#### Scenario: Coverage lists the problems of the index
- **WHEN** the index holds two problems, a connector call that failed and an external connector that waits for trust
- **THEN** the coverage text begins with `problems (2):` and one indented line for each, sorted, before the first connector
- **AND** its JSON holds both under `problems`, which is an empty list when the index holds none

### Requirement: Status answer
`status` SHALL say which gsttt version built the index, at which commit, how many files it holds, how many were dirty, how many were parsed and how many reused, how long each phase took, and how fresh it is.

#### Scenario: A stale index says what moved
- **WHEN** status is rendered for an index with freshness stale by two changed files and one added
- **THEN** the answer says that 2 files changed and 1 was added since the index was built

### Requirement: Answer text
Every answer SHALL name files by their path from the root and sites as `path:line`, SHALL stop each list at 30 entries with a line `… N more` unless every entry is asked for, SHALL say what was searched when it finds nothing, and SHALL end with one line giving, for each connector whose files the answer involves, the files read out of the files it owns and the share of imports resolved as a whole percentage rounded down, followed by one line of freshness.

#### Scenario: Long lists are cut with a count
- **WHEN** a file has 45 dependents
- **THEN** the text lists 30 of them followed by `… 15 more`
- **AND** with every entry asked for it lists all 45

#### Scenario: An empty answer says what was searched
- **WHEN** dependents of `src/x.ts` are asked, nothing imports it, the index holds 4213 files read by `ts` and `python`, and `src/y.ts` has the unresolved import `./x` at line 2
- **THEN** the answer says that nothing imports `src/x.ts` among 4,213 files read by ts and python
- **AND** it lists `src/y.ts:2` importing `./x` as a possible miss

#### Scenario: Every answer ends with coverage and freshness
- **WHEN** an answer is rendered from a fresh index built at commit `a1b2c3d4e5` with 3 dirty files
- **THEN** its last line is `fresh at a1b2c3d, 3 files dirty`
- **AND** the line before it gives the coverage of the connectors involved

#### Scenario: The resolved share is counted and rounded down
- **WHEN** an answer is rendered from an index where a connector found 100 imports and 29 of them resolved
- **THEN** the coverage line says `29% of imports resolved`

#### Scenario: A stale footer says so
- **WHEN** an answer is rendered from an index with freshness stale by two changed files
- **THEN** its last line starts with `stale:` and says that 2 files changed since the index

### Requirement: JSON answers
Every answer SHALL serialise to JSON holding everything its text holds, with no list cut.

#### Scenario: JSON keeps every entry
- **WHEN** an answer listing 45 dependents is serialised to JSON
- **THEN** the JSON holds all 45

### Requirement: Impact through names
Where the import edges name what the importer takes, `impact` SHALL carry a change name by name. A changed file and a file reached across a contract SHALL be affected in all their names, and an importer SHALL be affected in all its names when it imports by name an affected name of a file or a name the file does not export, and a file that re-exports such a name by name SHALL be affected in the names it exports it as.

#### Scenario: A changed barrel reaches every importer
- **WHEN** `index.ts` exports `own` and has `export * from './a'`, `form.ts` imports `fromA` from it, `page.ts` imports `own` from it, and `index.ts` changes
- **THEN** impact lists `form.ts` and `page.ts`

#### Scenario: A file that uses an affected name is affected in all its names
- **WHEN** `index.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` imports `fromA` from `a.ts` and exports `fromB`, `page.ts` imports `fromB` from `index.ts`, and `a.ts` changes
- **THEN** impact lists `b.ts` at distance 1 with the reason `imports fromA from a.ts`, `index.ts` at distance 1, and `page.ts` at distance 3 with the reason `imports fromB from index.ts`

#### Scenario: A name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `old.ts` imports `gone` from `index.ts`, nothing exports `gone`, and `a.ts` changes
- **THEN** impact lists `old.ts` with the reason `imports gone from index.ts`

#### Scenario: A re-export of a name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `outer.ts` exports `gone`, `kept` and `stable` and has `export { gone, kept } from './index'`, `page.ts` imports `gone` from `outer.ts`, `other.ts` imports `stable` from it, nothing else exports `gone`, and `a.ts` changes
- **THEN** impact lists `outer.ts` at distance 2 with the reason `re-exports gone, kept from index.ts` and `page.ts` at distance 3 with the reason `imports gone from outer.ts`
- **AND** it does not list `other.ts`

### Requirement: Names through re-exports
A named re-export SHALL affect the names it exports affected names as, and `export *` every affected name but `default`, where all the names of a file are those it exports itself and through its own `export *`. A file affected in some of its names only SHALL be listed with the reason that it re-exports them, each by the name the file it re-exports has for it, followed by `as` and the name it is exported as when that is another, so that a search of the file for what the reason names finds it.

#### Scenario: A barrel passes on only what it re-exports
- **WHEN** `index.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` exports `fromB`, `form.ts` imports `fromA` from `index.ts`, `page.ts` imports `fromB` from it, and `a.ts` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports fromA from a.ts` and `form.ts` at distance 2 with the reason `imports fromA from index.ts`
- **AND** it does not list `page.ts`

#### Scenario: A star does not forward default
- **WHEN** `index.ts` exports `default` and `own` and has `export * from './a'`, `a.ts` exports `default` and `fromA`, `form.ts` imports `fromA` from `index.ts`, `page.ts` imports `default` from it, and `a.ts` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports fromA from a.ts` and `form.ts` at distance 2
- **AND** it does not list `page.ts`, which takes the default export of `index.ts` itself

#### Scenario: A named re-export passes on its alias
- **WHEN** `index.ts` has `export { useFoo as useBar } from './foo'` and `export { other } from './other'`, `x.ts` imports `useBar` from `index.ts`, `y.ts` imports `other` from it, and `foo.ts` changes
- **THEN** impact lists `index.ts` with the reason `re-exports useFoo as useBar from foo.ts` and `x.ts` with the reason `imports useBar from index.ts`
- **AND** it does not list `y.ts`

#### Scenario: Chained barrels pass names on
- **WHEN** `app.ts` imports `fromA` from `index.ts`, `other.ts` imports `fromB` from it, `index.ts` has `export * from './inner'`, `inner.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` exports `fromB`, and `a.ts` changes
- **THEN** impact lists `inner.ts` at distance 1, `index.ts` at distance 2 and `app.ts` at distance 3
- **AND** it does not list `other.ts`

### Requirement: Star re-exports keep their importers
A file with `export *` SHALL be reached when the file it re-exports is affected, however few names that file still passes on, and none included, and it SHALL be listed with the reason that it re-exports the file. Its importers SHALL then be measured against the names it exports like those of any other barrel, so that a name nothing exports any more is affected. This SHALL hold on every hop of a chain of star re-exports.

#### Scenario: A file that exports nothing more still reaches the importers of its barrel
- **WHEN** `index.ts` exports nothing itself and has `export * from './only'`, `only.ts` exported `foo` and exports nothing now, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports only.ts` and `page.ts` at distance 2 with the reason `imports foo from index.ts`

#### Scenario: A file left with only a default export still reaches the importers of its barrel
- **WHEN** `index.ts` exports nothing itself and has `export * from './only'`, `only.ts` exports only `default`, which `export *` never forwards, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports only.ts` and `page.ts` at distance 2 with the reason `imports foo from index.ts`

#### Scenario: A star barrel behind a star barrel reaches the importers of both
- **WHEN** `mid.ts` has `export * from './only'`, `index.ts` has `export * from './mid'`, neither exports anything itself, `only.ts` exports nothing now, `page.ts` imports `foo` from `index.ts`, and `only.ts` changes
- **THEN** impact lists `mid.ts` at distance 1, `index.ts` at distance 2 with the reason `re-exports mid.ts`, and `page.ts` at distance 3 with the reason `imports foo from index.ts`

### Requirement: Unknown exports mean the whole file
An edge into a file whose exports are unknown SHALL be followed at file level, whatever names it carries: its importer SHALL be affected in all its names, with the reason that it imports the file and no names, whether it imports by name, passes names on or re-exports everything. A file that re-exports everything from a file with unknown exports has unknown exports too. A connector that lists no exports SHALL therefore give the files, distances and reasons of a walk over files.

#### Scenario: A named import of a file with unknown exports is followed at file level
- **WHEN** `user.py` imports `parse` from `lib.py`, the exports of `lib.py` are unknown, and `lib.py` changes
- **THEN** impact lists `user.py` at distance 1 with the reason `imports lib.py`, which is what it lists when `user.py` imports `lib.py` without names

#### Scenario: A star re-export of a file with unknown exports is followed at file level
- **WHEN** `index.py` re-exports everything from `lib.py`, the exports of `lib.py` are unknown, `app.py` imports `parse` from `index.py`, and `lib.py` changes
- **THEN** impact lists `index.py` at distance 1 with the reason `imports lib.py` and `app.py` at distance 2 with the reason `imports index.py`, which is what it lists when each of them imports its file without names

#### Scenario: A named re-export of a file with unknown exports is followed at file level
- **WHEN** `index.ts` exports `thing` with `export { thing } from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `imports legacy.js`, as it does when `index.ts` imports `legacy.js` without names, and `page.ts` at distance 2 with the reason `imports thing from index.ts`

#### Scenario: A barrel that re-exports a file with unknown exports has unknown exports too
- **WHEN** `index.ts` exports `own` and has `export * from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes
- **THEN** impact lists `index.ts` at distance 1 with the reason `imports legacy.js` and `page.ts` at distance 2 with the reason `imports index.ts`

### Requirement: Imports that take every name
An importer that imports a namespace, imports for side effects, dynamically, by `require`, or with names its connector does not record SHALL be affected in all its names as soon as any name of the file is affected, and so SHALL a file whose own exports are unknown.

#### Scenario: A namespace import takes every name
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `fromA`, `all.ts` imports `index.ts` as a namespace, `side.ts` imports it for its side effects, and `a.ts` changes
- **THEN** impact lists `all.ts` and `side.ts`, each with the reason `imports index.ts`

#### Scenario: Unknown exports keep every importer
- **WHEN** `index.ts` has `export * from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes
- **THEN** impact lists `index.ts` and `page.ts`

### Requirement: Contracts from whole files
Only a file affected in all its names SHALL cross its contracts.

#### Scenario: A barrel reached for some names crosses no contract
- **WHEN** `index.ts` has `export * from './a'` and serves `http GET /health`, `a.ts` exports `fromA`, `client.ts` calls `http GET /health`, and `a.ts` changes
- **THEN** impact lists `index.ts` and does not list `client.ts`

### Requirement: Names in answers
An answer SHALL write the names of an edge in byte order, the first three followed by `and N more` unless every entry is asked for, and its JSON SHALL hold every name.

#### Scenario: Names are cut after three
- **WHEN** `x.ts` imports `one`, `two`, `three` and `four` from `a.ts`, `y.ts` imports `one` from it, and `a.ts` changes
- **THEN** the impact text gives `x.ts` the reason `imports four, one, three and 1 more from a.ts` and `y.ts` the reason `imports one from a.ts`
- **AND** with every entry asked for the reason of `x.ts` is `imports four, one, three, two from a.ts`, and its JSON reason holds all four names

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
Asked about names, `impact` SHALL take one hop unless the request gives a depth. Each further hop SHALL go from the declarations reached to the declarations that refer to them, by the rules of the first. When the depth stops the walk with callers left to follow, the answer SHALL say so, and that asking about a caller or with a larger depth goes on.

#### Scenario: A larger depth follows the callers of callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` imports `run` from `b.ts` and uses it in `main`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts` at distance 1 with the caller `run` and `c.ts` at distance 2 with the reason `imports run from b.ts` and the caller `main`

#### Scenario: The answer says when the walk stopped at the callers
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` uses `run` in `main`, and impact is asked about `foo` in `a.ts` without a depth
- **THEN** the answer lists `b.ts` and not `c.ts`, and has a note that the walk stopped with callers left to follow

#### Scenario: A walk that runs out says nothing about stopping
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, nothing refers to `run`, and impact is asked about `foo` in `a.ts`
- **THEN** the answer lists `b.ts` and has no note about stopping

### Requirement: Distance of a caller
A file SHALL be listed once, at the distance the walk first reached it, and a caller or a load of it that the walk reached farther SHALL say its own distance, in the text and in the JSON.

#### Scenario: A caller reached farther than its file says its distance
- **WHEN** `b.ts` uses `foo` of `a.ts` in `inner`, which starts at line 1, `outer`, which starts at line 5, uses `inner`, the load of `b.ts` uses `inner`, and impact is asked about `foo` in `a.ts` with depth 2
- **THEN** impact lists `b.ts  distance 1  imports foo from a.ts; in inner (line 1), outer (line 5, distance 2); at load (distance 2)`
- **AND** the JSON entry of `b.ts` has the callers `inner` at distance 1 and `outer` at distance 2, and its load at distance 2
- **AND** asked with depth 1, impact lists `b.ts  distance 1  imports foo from a.ts; in inner (line 1)`

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
