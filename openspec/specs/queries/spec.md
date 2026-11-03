# queries Specification

## Purpose
Queries read an index and answer what a change affects and how files and contracts connect, as short text for people and agents and as JSON for scripts. Every answer says what it searched, how much of the tree was read and how fresh the index is.

## Requirements

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

### Requirement: Impact across contracts
From every changed and every affected file, `impact` SHALL cross the contracts the file has sites of: a `declares` site reaches every file that serves or calls the contract, a `serves` site reaches every file that calls it, and a `calls` site reaches no file. An HTTP contract SHALL be crossed to the sites of every key that meets it, and a GraphQL contract only between sites that meet in an endpoint. The reason of a file reached across a contract SHALL be its own role and key.

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
`path` SHALL return a shortest chain from one file to another, where a step is an import, a hop from a file that calls a contract to a file that serves or declares it, or a hop from a file that serves a contract to a file that declares it. HTTP hops SHALL follow keys that meet, and GraphQL hops sites that meet in an endpoint. When there is no chain, the answer SHALL say so and how many files are reachable from the first file.

#### Scenario: A chain runs through a contract
- **WHEN** `page.tsx` imports `api.ts`, `api.ts` calls `http GET /bookings/{}`, and `bookings.controller.ts` serves it
- **THEN** the path from `page.tsx` to `bookings.controller.ts` is `page.tsx`, `api.ts` by import, `bookings.controller.ts` by `http GET /bookings/{}`

#### Scenario: No chain is reported with its reach
- **WHEN** a path is asked between two files with no chain between them
- **THEN** the answer says there is none and how many files the first one reaches

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

#### Scenario: Asking about a GET finds a fetch that writes no method
- **WHEN** `GET /x` is looked up, and a TypeScript file has `fetch('/x')`
- **THEN** the answer lists `http GET /x` with one call
- **AND** looking up `POST /x` lists no key

### Requirement: HTTP matching
Two HTTP keys SHALL meet when their methods are equal or either is `*`, and their paths have the same number of segments, each pair equal or `{}` on either side, with or without one prefix from the index's `http_prefixes` removed from the start of one of the two paths. A prefix SHALL NOT be removed from both paths, and SHALL NOT be removed from a path that it is all of. When both paths hold a literal segment, at least one pair SHALL be the same literal on both sides.

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

#### Scenario: Literals that meet only parameters make no match
- **WHEN** `http POST /{}/messages` is compared with `http POST /u/{}`
- **THEN** they do not meet

#### Scenario: A path of parameters alone meets a literal path
- **WHEN** `http GET /{}` is compared with `http GET /users`, and `http GET /{}/{}` with `http GET /a/b`
- **THEN** both pairs meet

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
- **WHEN** `index.ts` has `export * from './a'` and `export * from './b'`, `a.ts` exports `fromA`, `b.ts` imports `fromA` from `a.ts` and exports `fromB`, `page.ts` imports `fromB` from `index.ts`, and `a.ts` changes with depth 3
- **THEN** impact lists `b.ts` at distance 1 with the reason `imports fromA from a.ts`, `index.ts` at distance 1, and `page.ts` at distance 3 with the reason `imports fromB from index.ts`

#### Scenario: A name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `old.ts` imports `gone` from `index.ts`, nothing exports `gone`, and `a.ts` changes with depth 2
- **THEN** impact lists `old.ts` with the reason `imports gone from index.ts`

#### Scenario: A re-export of a name the barrel does not export is affected
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `kept`, `outer.ts` exports `gone`, `kept` and `stable` and has `export { gone, kept } from './index'`, `page.ts` imports `gone` from `outer.ts`, `other.ts` imports `stable` from it, nothing else exports `gone`, and `a.ts` changes with depth 3
- **THEN** impact lists `outer.ts` at distance 2 with the reason `re-exports gone, kept from index.ts` and `page.ts` at distance 3 with the reason `imports gone from outer.ts`
- **AND** it does not list `other.ts`

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

### Requirement: Imports that take every name
An importer that imports a namespace, imports for side effects, dynamically, by `require`, or with names its connector does not record SHALL be affected in all its names as soon as any name of the file is affected, and so SHALL a file whose own exports are unknown.

#### Scenario: A namespace import takes every name
- **WHEN** `index.ts` has `export * from './a'`, `a.ts` exports `fromA`, `all.ts` imports `index.ts` as a namespace, `side.ts` imports it for its side effects, and `a.ts` changes with depth 2
- **THEN** impact lists `all.ts` and `side.ts`, each with the reason `imports index.ts`

#### Scenario: Unknown exports keep every importer
- **WHEN** `index.ts` has `export * from './legacy'`, the exports of `legacy.js` are unknown, `page.ts` imports `thing` from `index.ts`, and `legacy.js` changes with depth 2
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
- **THEN** the text starts with `impact of foo in a.ts, within 1 hop through code` and lists `b.ts  distance 1  imports foo from a.ts; in run (line 3), stop (line 7)`
- **AND** the JSON entry of `b.ts` has the callers `run` at line 3 and `stop` at line 7

### Requirement: Members of a class
A member of a class SHALL be a declaration of its own, `Class.member`, so that a name used in one method affects that method and not its class. Asked about a member, `impact` SHALL list the members that use it through `this` and the declarations that use the class where their file reads the member by name, and SHALL count the files that only hold the class in one note.

#### Scenario: A name used in a method affects that method
- **WHEN** `runner.ts` declares the class `Runner` with the method `start`, which calls `foo` imported from `a.ts`, and the method `stop`, `app.ts` imports `Runner` and uses it in `main`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.start`
- **AND** it does not list `Runner`, `Runner.stop` or `app.ts`

#### Scenario: Callers of a member are found through this and through its class
- **WHEN** `Runner.stop` calls `this.start()`, `app.ts` imports `Runner` and uses it in `main`, which reads `start`, and impact is asked about `Runner.start` in `runner.ts`
- **THEN** impact lists `runner.ts` with the caller `Runner.stop` and `app.ts` with the caller `main`
- **AND** the answer has no note

#### Scenario: Members of several classes are one note
- **WHEN** `runner.ts` declares the class `Runner` with the methods `start` and `stop` and the class `ModuleRunner` with the method `import`, `app.ts` imports `Runner` and uses it in `main`, which reads none of them, and impact is asked about `Runner.start`, `Runner.stop` and `ModuleRunner.import` in `runner.ts`
- **THEN** impact lists no file
- **AND** the answer has one note, that 1 file holds the classes of `ModuleRunner.import`, `Runner.start` and `Runner.stop` and names none of them, and that asking about those classes lists it

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

### Requirement: Types outside declarations
Asked about names, an import type SHALL be a caller only in the declarations whose lines hold it. Outside every declaration it SHALL reach nothing, because a type does not run as the file loads.

#### Scenario: An import type outside every declaration reaches nothing
- **WHEN** `a.ts` declares `foo`, `a.test.ts` has `vi.mock('./a', async (importOriginal) => ({ ...(await importOriginal<typeof import('./a')>()) }))` at its top level and imports nothing else from `a.ts`, `types.ts` has `export type Api = typeof import('./a')`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `types.ts` with the caller `Api`
- **AND** it does not list `a.test.ts`

### Requirement: A hop across a contract is no hop through code
The depth of `impact` SHALL bound the hops through code: imports, uses of a name, loads and mocks. A hop across a contract SHALL count toward the distance of a file and not toward the depth, so a file reached across a contract is followed through code as far as any other, in the walk by name and in the walk over files, and it carries the fewest hops through code of the files that reach it. The first line of an impact answer SHALL say `within N hop(s) through code`.

#### Scenario: A client of a route a caller serves is reached within the default depth
- **WHEN** `db.ts` declares `rows`, `api.ts` imports it and uses it in `listUsers`, which serves `http GET /users`, `users.ts` calls `http GET /users` in `fetchUsers`, `page.ts` imports `fetchUsers` and uses it in `Page`, and impact is asked about `rows` in `db.ts` without a depth
- **THEN** impact lists `api.ts` at distance 1 with the caller `listUsers` and `users.ts` at distance 2 with the reason `calls http GET /users` and the caller `fetchUsers`
- **AND** it does not list `page.ts`, and has a note that the walk stopped with callers left to follow

#### Scenario: A file reached across a contract is followed through code
- **WHEN** a controller serves `http GET /bookings/{}`, `api.ts` calls it, `page.tsx` imports `api.ts`, `app.tsx` imports `page.tsx`, and the controller changes with depth 1
- **THEN** impact lists `api.ts` at distance 1 and `page.tsx` at distance 2
- **AND** it does not list `app.tsx`

#### Scenario: A contract is crossed with the fewest hops through code of the servers, by name
- **WHEN** `lib.ts` declares `foo`, which serves `http GET /y`, `a.ts` imports `foo` and serves `http GET /x` in `pa`, `b.ts` calls `http GET /y` and serves `http GET /x` in `qb`, `c.ts` calls `http GET /x` in `cc`, `d.ts` imports `cc` from `c.ts` and uses it in `dd`, and impact is asked about `foo` in `lib.ts` without a depth
- **THEN** impact lists `a.ts` and `b.ts` at distance 1, `c.ts` at distance 2 with the caller `cc`, and `d.ts` at distance 3 with the caller `dd`

#### Scenario: A contract is crossed with the fewest hops through code of the servers, over files
- **WHEN** `ch.ts` serves `http GET /y`, `a.ts` imports `ch.ts` and serves `http GET /x`, `b.ts` calls `http GET /y` and serves `http GET /x`, `c.ts` calls `http GET /x`, `d.ts` imports `c.ts`, and `ch.ts` changes with depth 1
- **THEN** impact lists `a.ts` and `b.ts` at distance 1, `c.ts` at distance 2 and `d.ts` at distance 3

#### Scenario: An import found second gives the fewer hops through code, over files
- **WHEN** `r.ts` serves `http GET /x`, `a.ts` imports `r.ts`, `z.ts` calls `http GET /x`, `m.ts` imports `a.ts` and `z.ts`, `n.ts` imports `m.ts`, and `r.ts` changes with depth 2
- **THEN** impact lists `a.ts` and `z.ts` at distance 1, `m.ts` at distance 2 with the reason `imports a.ts`, and `n.ts` at distance 3

#### Scenario: An import found after a contract gives the fewer hops through code, over files
- **WHEN** `r.ts` serves `http GET /x`, `b0.ts` imports `r.ts`, `b.ts` imports `b0.ts` and serves `http GET /y`, `z1.ts` calls `http GET /x` and serves `http GET /w`, `z2.ts` calls `http GET /w`, `x.ts` calls `http GET /y` and imports `z2.ts`, `w.ts` imports `x.ts`, `v.ts` imports `w.ts`, and `r.ts` changes with depth 3
- **THEN** impact lists `x.ts` at distance 3 with the reason `calls http GET /y`, `w.ts` at distance 4 and `v.ts` at distance 5

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

### Requirement: GraphQL endpoints
With endpoints in the index, `impact` and `path` SHALL cross a GraphQL contract only between sites that meet. A site belongs to the endpoints whose globs match its file: a `serves` site by `servers`, a `declares` site by `clients` or `servers`, a `calls` site by `clients` or, when none matches, by `servers`. Two sites meet when the one in the higher role (`calls`, `serves`, `declares`) belongs to no endpoint, or the two share one. Other contracts and the `contract` query SHALL NOT be scoped.

#### Scenario: A change reaches the callers of its own endpoint only
- **WHEN** the endpoint `a` has the clients `web-a/**` and the servers `api-a/**`, the endpoint `b` has `web-b/**` and `api-b/**`, `api-a/users.ts` and `api-b/users.ts` serve `graphql Query.user`, `web-a/page.ts` and `web-b/page.ts` call it, and `api-a/users.ts` changes
- **THEN** impact lists `web-a/page.ts` with the reason `calls graphql Query.user` and does not list `web-b/page.ts`

#### Scenario: A chain runs within an endpoint only
- **WHEN** the endpoints and files above are given and a path is asked from `web-a/page.ts`
- **THEN** the path to `api-a/users.ts` is `web-a/page.ts` and `api-a/users.ts`, and there is no path to `api-b/users.ts`

#### Scenario: A schema reaches the sites of its own endpoint only
- **WHEN** the endpoints and files above are given, `api-a/schema.graphql` and `api-b/schema.graphql` declare `graphql Query.user`, and `api-a/schema.graphql` changes
- **THEN** impact lists the server and the page of `a` and no file of `b`
- **AND** a path from `web-b/page.ts` reaches the schema of `b` and not the schema of `a`

#### Scenario: A site in no endpoint meets by the role it has
- **WHEN** the endpoints and files above are given, and `scripts/probe.ts` calls `graphql Query.user` and `legacy/users.ts` serves it, both matching no glob
- **THEN** a change of `api-a/users.ts` reaches `scripts/probe.ts`, and a path from `scripts/probe.ts` reaches the servers of both endpoints and `legacy/users.ts`
- **AND** a change of `legacy/users.ts` reaches `scripts/probe.ts` and no page of an endpoint, and no path leads from `web-a/page.ts` to `legacy/users.ts`

#### Scenario: An endpoint served outside the repository keeps its callers apart
- **WHEN** the endpoint `a` has the clients `web-a/**` and no servers, `web-a/page.ts` calls `graphql Query.user`, `api/users.ts` serves it and matches no glob, and `api/users.ts` changes
- **THEN** impact does not list `web-a/page.ts`

#### Scenario: A file that serves one endpoint and calls another crosses both
- **WHEN** the first endpoint has the clients `web/**` and the servers `gateway/**` and `schema-a/**`, the second has the clients `gateway/**` and the servers `api/**`, `gateway/graph.ts` serves and calls `graphql Query.user`, `schema-a/user.graphql` declares it, `api/users.ts` serves it, `web/page.ts` calls it, and `entry.ts` imports the gateway
- **THEN** a path from `entry.ts` reaches `schema-a/user.graphql` and `api/users.ts`, each through the gateway
- **AND** a path from `web/page.ts` reaches `api/users.ts` through the gateway, and a change of `api/users.ts` reaches the gateway and `web/page.ts`

#### Scenario: Without endpoints a GraphQL contract meets every site
- **WHEN** the files of the first scenario are given and the index has no endpoints
- **THEN** a change of `api-a/users.ts` reaches both pages, and a path from `web-a/page.ts` reaches `api-b/users.ts`

#### Scenario: Endpoints do not scope other contracts
- **WHEN** the endpoints and files of the first scenario serve and call `http GET /user` in place of the GraphQL key
- **THEN** a change of `api-a/users.ts` reaches both pages

#### Scenario: The contract query lists every site whatever the endpoints
- **WHEN** the endpoints and files of the first scenario are given and `contract` is asked for `graphql Query.user`
- **THEN** it lists `api-a/users.ts` and `api-b/users.ts` as servers and `web-a/page.ts` and `web-b/page.ts` as callers

#### Scenario: A walk by name crosses a GraphQL contract within an endpoint
- **WHEN** the endpoints and files of the first scenario are given, `lib/user.ts`, which matches no glob, declares `loadUser`, and `api-a/users.ts` uses it in the resolver that serves `graphql Query.user`
- **THEN** impact asked about `loadUser` lists `api-a/users.ts` and `web-a/page.ts` and not `web-b/page.ts`

### Requirement: A file that sets and reads a contract
A file that both serves and calls a contract SHALL leave it as a server, so that a change to it reaches the files that call the contract and a change to a file that only calls it does not reach the file that serves it.

#### Scenario: A file that sets a variable reaches the files that read it
- **WHEN** `a.py` serves and calls `env X`, `b.py` calls `env X`, and `a.py` changes
- **THEN** impact lists `b.py` with the reason `calls env X`, and a change to `b.py` does not list `a.py`

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

### Requirement: A caller is told apart by the member it names
Asked about a member, `impact` SHALL NOT list a declaration whose only uses of the class name other members, whether it reaches them through a new instance, a local value, a parameter or the class itself. A declaration that takes the members of the class, as a subclass or a value made by `new` or by a factory does, SHALL NOT be listed for naming it.

#### Scenario: A caller of another member through a new instance is not listed
- **WHEN** `service.ts` declares `OrderService` with `place` and `cancel`, `refunds.ts` has `new OrderService().cancel(id)` in `refund`, `checkout.ts` has `new OrderService().place(id)` in `pay`, and impact is asked about `OrderService.place` in `service.ts`
- **THEN** impact lists `checkout.ts` with the caller `pay`
- **AND** it does not list `refunds.ts`

#### Scenario: Asked about the class, every caller of a member is listed
- **WHEN** the files are those of the previous scenario, with also `keep.ts` declaring `function keep(service: OrderService) {}`, and impact is asked about `OrderService`
- **THEN** impact lists `checkout.ts`, `refunds.ts` and `keep.ts`

#### Scenario: A subclass that names the member only on another value is not a caller
- **WHEN** `sub.ts` has `Sub extends Base`, `run(socket)` calls `socket.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact does not list `sub.ts`
- **AND** the note counts it as a file that holds `Base` and names no `destroy`

#### Scenario: A subclass that reads the member through this is a caller
- **WHEN** `Sub extends Base` has a `run` that calls `this.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact lists the file of `Sub` with the caller `Sub.run`

#### Scenario: A top-level value made by new is a caller at its reader and not at its declarator
- **WHEN** `store.ts` has `const shared = new Base()` and `stop` calls `shared.destroy()`, and impact is asked about `Base.destroy`
- **THEN** impact lists `store.ts` with the caller `stop` and not `shared`

### Requirement: A changed declaration is walked with its file
An `impact` request that names the changed declarations of a file SHALL walk those declarations and the declarations of the file that use them, and a name that is empty SHALL walk nothing. The header SHALL name the declarations and the file.

#### Scenario: A changed declaration is walked with the declarations of its file that use it
- **WHEN** `helper` of `lib.ts` is asked for, `render` uses `helper`, `a.ts` imports `render` and `b.ts` imports `unrelated`
- **THEN** only `a.ts` is affected and the header is `impact of helper in lib.ts, within 1 hop through code`

### Requirement: A star barrel names nothing
Asked about a name, a file that passes it on by `export *` SHALL get no row, because nothing in it names the name; the walk SHALL go through it with no hop and reach its importers as before. A file that re-exports a class, `export * as ns` or hands a class on by `module.exports.A = A` SHALL get none either for a member of the class or a name of the namespace. A file that names what it passes on keeps its row.

#### Scenario: A star barrel gets no row when a name is asked about
- **WHEN** `index.ts` has `export * from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` at distance 1 with the reason `imports helper from index.ts`, and no other file

#### Scenario: A chain of star barrels gets no row
- **WHEN** `index.ts` has `export * from './a'`, `outer.ts` has `export * from './index'`, a test imports `helper` from `outer.ts`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists the test at distance 1, and neither barrel

#### Scenario: A named re-export keeps its row when a name is asked about
- **WHEN** `index.ts` has `export { helper } from './a'`, `page.ts` imports `helper` from `index.ts` and uses it in `open`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `index.ts` at distance 1 with the reason `re-exports helper from a.ts`, and `page.ts` at distance 1

#### Scenario: A re-export of a class gets no row for a member of it
- **WHEN** `index.ts` has `export { Service } from './service'`, `page.ts` imports `Service` from `index.ts` and its `go` uses `Service.place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists `page.ts` and not `index.ts`
- **AND** asked about `Service`, impact lists both

#### Scenario: A namespace re-export gets no row for a name it holds
- **WHEN** `index.ts` has `export * as lib from './a'`, `page.ts` imports `lib` from `index.ts` and its `go` uses `lib.helper`, and impact is asked about `helper` in `a.ts`
- **THEN** impact lists `page.ts` and not `index.ts`

#### Scenario: A file that hands a class on gets no row for a member of it
- **WHEN** `mid.js` exports a class of `base.js` under another name by `module.exports.Agent = Base`, `page.js` imports it and calls `Agent.destroy`, and impact is asked about `Base.destroy`
- **THEN** impact lists `page.js` and not `mid.js`
- **AND** asked about `Base`, impact lists both

### Requirement: Members that pass on
Asked about a member, `impact` SHALL go on to each name that takes the members of its class and does not declare that member, through the heirs of the heirs, in the same hop. An importer of such a name SHALL be a caller when it uses `Heir.member` or the heir alone. A private member SHALL not pass on. Asked about a class as a whole, `impact` SHALL list what it listed before.

#### Scenario: A caller through a subclass is a caller of the member
- **WHEN** `base.ts` declares `Base` with `place` and `cancel`, `sub.ts` declares `Sub`, which extends `Base`, `a.ts` declares `run`, which uses `Sub.place`, `b.ts` declares `stop`, which uses `Sub.cancel`, and impact is asked about `Base.place` in `base.ts`
- **THEN** impact lists `a.ts` at distance 1 with the caller `run`
- **AND** it does not list `b.ts`

#### Scenario: A subclass that declares the member keeps its own callers
- **WHEN** `Sub` extends `Base` and declares `place`, `a.ts` uses `Sub.place`, and impact is asked about `Base.place`
- **THEN** impact lists `sub.ts` with the caller `Sub.place`
- **AND** it does not list `a.ts`

#### Scenario: A member goes through a chain of subclasses
- **WHEN** `Sub2` extends `Sub`, which extends `Base`, in a third file and within one file, a declaration of another file uses `Sub2.place`, and impact is asked about `Base.place`
- **THEN** impact lists that file at distance 1 with that declaration as its caller

#### Scenario: A top-level value of a class is a caller's way to the member
- **WHEN** `store.ts` has `orders` as an heir of `OrderService`, `page.ts` declares `render`, which uses `orders.place`, `other.ts` declares `clear`, which uses `orders.cancel`, and impact is asked about `OrderService.place`
- **THEN** impact lists `page.ts` at distance 1 with the caller `render`
- **AND** it does not list `other.ts`

#### Scenario: A subclass in the file of its base
- **WHEN** `filemap.ts` declares `FileMap` with `set`, `ModuleMap`, which extends it, and `build`, which uses `ModuleMap.set`, `user.ts` imports `ModuleMap` and declares `load`, which uses `ModuleMap.set`, and impact is asked about `FileMap.set`
- **THEN** impact lists `user.ts` at distance 1 with the caller `load`
- **AND** it lists `build` among the callers in `filemap.ts`

#### Scenario: A private member does not pass on
- **WHEN** `Base` declares `#secret`, `Sub` extends it, `a.ts` uses `Sub.#secret`, and impact is asked about `Base.#secret`
- **THEN** impact lists no file

#### Scenario: A member goes on through a namespace
- **WHEN** `sub.ts` has `import * as lib from './lib'` and `Sub` extends `lib.Base`, `a.ts` uses `Sub.place`, `b.ts` uses `Sub.cancel`, and impact is asked about `Base.place` in `lib.ts`
- **THEN** impact lists `a.ts` and not `b.ts`

### Requirement: Go answers name declarations
A Go function or method SHALL be a name that `impact --name` asks about: the answer lists the files that call it and the declarations in them, and a caller of another function or another method of the same type is not listed.

#### Scenario: A Go caller of a function is listed and a caller of another function is not
- **WHEN** `invoice/header.go` declares `Header`, `invoice/footer.go` declares `Footer`, `report.go` calls `invoice.Header()` in `Print` and `ship.go` calls `invoice.Footer()`, and `impact invoice/header.go --name Header` runs
- **THEN** the answer lists `report.go` with the caller `Print`, and does not list `ship.go`

#### Scenario: A Go caller of a method is told apart from a caller of another method
- **WHEN** type `Service` has the methods `Place` and `Cancel`, `Order` takes a `*invoice.Service` and calls `Place`, `Refund` takes one and calls `Cancel`, and `impact invoice/service.go --name Service.Place` runs
- **THEN** the callers are `Order` only
- **AND** with `--name Service.Cancel` they are `Refund` only

#### Scenario: A Go method declared in a file of its own is found through its type
- **WHEN** `invoice/charge.go` holds only `func (s *Service) Charge() {}`, `invoice/service.go` declares `Service`, `order.go` calls `Charge` on an `*invoice.Service`, and `impact invoice/charge.go --name Service.Charge` runs
- **THEN** the answer lists `order.go`

#### Scenario: A Go client meets a route served in another language
- **WHEN** an Express app serves `GET /api/users` and a Go file calls `http.Get(base + "/api/users")`
- **THEN** `contract "http GET /api/users"` lists the Express file as serving and the Go file as calling
- **AND** `impact` of the Express file lists the Go file

### Requirement: A method through what a function returns
Asked about a Go method, `impact` SHALL list a file that calls it on a value a function returns, on a type that embeds the type of the method, or on a package-level variable, and SHALL not list a file that reads another method there. A file that names the type and reads no such method SHALL be counted in a note.

#### Scenario: A method called on what a function returns reaches the caller
- **WHEN** `lib/new.go` declares `func NewRouter() *Mux`, `app/uses.go` calls `lib.NewRouter().Get` through a local, `app/other.go` does the same with `Put`, and impact is asked about `Mux.Get` in `lib/mux.go`
- **THEN** impact lists `app/uses.go` and not `app/other.go` or `lib/new.go`
- **AND** a note counts the two files that hold `Mux` and name no `Get`

#### Scenario: A method promoted through embedding reaches the caller
- **WHEN** `Engine` embeds `RouterGroup`, `func New() *Engine` is in the same file, a file of another package calls `e := lib.New(); e.PATCH()`, and impact is asked about `RouterGroup.PATCH`
- **THEN** impact lists that file

#### Scenario: A method through a package variable reaches the caller
- **WHEN** `var V2 = v2{}` is declared, a file calls `lib.V2.Make()`, and impact is asked about `v2.Make`
- **THEN** impact lists that file

### Requirement: Impact through a package required in place
Impact SHALL reach the caller of a route that a file serves through a router made by a package required in place and mounted with a module required in place.

#### Scenario: A router required in place reaches a fetch of its route
- **WHEN** `a.js` has `require('express').Router()` with `router.get('/a', h)`, `b.js` mounts `require('./a')` at `/api`, and a TypeScript file calls `fetch('/api/a')`
- **THEN** impact of `a.js` lists the TypeScript file

### Requirement: Names a CommonJS file writes in place
Asked about a name that a CommonJS file writes in place on `exports` or `module.exports`, impact SHALL list the files that read that name from it, and no file that reads only another name of the file.

#### Scenario: A name written in place lists the files that read it
- **WHEN** `lib.js` has `exports.run = function () {}` and `exports.stop = function () {}`, `runs.js` has `const lib = require('./lib')` and `lib.run()` at its top level, `stops.js` reads only `lib.stop()`, and impact is asked about `run` in `lib.js`
- **THEN** impact lists `runs.js` and not `stops.js`

### Requirement: Impact through a destructured name
Impact of a name SHALL list a file that takes the name out of a module by destructuring, and SHALL leave out a file that destructures only other names of the module.

#### Scenario: A destructured name lists the files that take it
- **WHEN** `lib.js` exports `a` and `other`, and `taker.js` has `const lib = require('./lib')` and `const { a } = lib`
- **THEN** impact of `lib.js` for `a` lists `taker.js`, and for `other` does not

### Requirement: Callers through a head
Asked about a member of a class, or run with `--changed` from one, `impact` SHALL also list the callers of the same member of each class or interface the class extends or implements, and of their heads in turn, found through imports and barrels. A holder SHALL count only when its file names the member. A head outside the repository, or one that does not declare the member, SHALL add nothing. A note SHALL name the member and the head.

#### Scenario: A caller through an interface is a caller of the method that implements it
- **WHEN** `GitProvider` declares `getGitGroups`, `Aws` implements it and declares it, `app.ts` holds a `GitProvider` and calls `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact lists `app.ts` with the caller that calls it
- **AND** it says that `Aws.getGitGroups` may run through `GitProvider.getGitGroups`

#### Scenario: A caller through a base class is a caller of the method that overrides it
- **WHEN** `Base` declares `run`, `Sub` extends it and overrides `run`, `app.ts` holds a `Base` and calls `run`, and impact is asked about `Sub.run`
- **THEN** impact lists `app.ts`

#### Scenario: A sibling implementation is no caller
- **WHEN** `Github` also implements `GitProvider`, a file holds only `Github` and names `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact does not list that file

#### Scenario: A method the interface does not declare gets nothing from it
- **WHEN** `Aws` implements `GitProvider`, declares `extra`, which `GitProvider` does not declare, a file holds a `GitProvider` and names `extra`, and impact is asked about `Aws.extra`
- **THEN** impact lists no file and says nothing about a head

#### Scenario: A head is followed through a barrel
- **WHEN** `Aws` implements a `GitProvider` it imports with `import type` from a barrel that re-exports it by name, or by `export *`, and a file holds the `GitProvider` and names the method
- **THEN** impact asked about `Aws.getGitGroups` lists that file

#### Scenario: A changed method is followed through what it implements
- **WHEN** `Aws.getGitGroups` changed in the working tree and `app.ts` holds a `GitProvider` and calls `getGitGroups`
- **THEN** `impact --changed` lists `app.ts`

### Requirement: Members of an interface can be asked about
Asked about `Interface.member`, `impact` SHALL list the files that hold the interface and name the member.

#### Scenario: Callers of a member of an interface are the holders that name it
- **WHEN** `GitProvider` declares `getGitGroups` and `other`, `app.ts` holds a `GitProvider` and names `getGitGroups`, `rest.ts` holds one and names `other`, and impact is asked about `GitProvider.getGitGroups`
- **THEN** impact lists `app.ts`
- **AND** it does not list `rest.ts`

### Requirement: Impact through a router of another package
A change to a router in one package SHALL reach the files that call the paths the router serves under the mounts of another package, because the router serves each route under the path an application mounts it at.

#### Scenario: A router of another package reaches the client of the mounted path
- **WHEN** an app of one package mounts a router of a second package at `/ext`, a client of a third package calls `/ext/x`, and the router file is the changed path
- **THEN** the affected files are the app and the client, and the client's reason is a call of `http GET /ext/x` through a contract

### Requirement: Callers through what a factory gives
Asked about a member, `impact` SHALL list the file that gets a value of the class from a static call on it or from a function that gives it, and reads that member on the value, and a file that reads the member on a value that a third file passes on. A file that calls the factory and reads no such member SHALL NOT be listed.

#### Scenario: A caller through a static factory and through a function factory is listed
- **WHEN** `service.ts` declares `Service` with `place`, a static `make` and `makeService(): Service`, `by-static.ts` has `const s = Service.make(); s.place()` and `by-function.ts` has `const s = makeService(); s.place()`, and impact is asked about `Service.place`
- **THEN** impact lists `by-static.ts` and `by-function.ts`

#### Scenario: A file that calls a factory and reads no such member is not listed
- **WHEN** a file has `Service.make().other()` and `makeService().other()`, and another only `makeService()`
- **THEN** impact asked about `Service.place` lists neither

#### Scenario: A value made by a factory and passed across a third file is a caller
- **WHEN** `service.ts` has `export const made = makeService()`, `third.ts` has `export { made } from './service'` and `shared.ts` has `made.place()`
- **THEN** impact asked about `Service.place` lists `shared.ts`

### Requirement: Impact through a computed export key
Impact SHALL NOT pass a name a file only imports on to the files that require it because the file assigns a computed key of its exports at the top level, and SHALL still list the file that takes a name that key hands out.

#### Scenario: A computed export key does not pass an imported name on
- **WHEN** `a.js` imports a class, calls it in `run`, has `module.exports = run` and `module.exports[Symbol.for('x')] = { helper }`, `b.js` requires `./a`, and `c.js` has `require('./a')[Symbol.for('x')]`
- **THEN** impact of the class lists `a.js` and not `b.js`
- **AND** impact of `helper` in `a.js` lists `c.js`

### Requirement: Overrides are listed
Asked about a member, `impact` SHALL list, in the file that declares it, the same member of each class that extends or implements its class, at any depth, with the reason `overrides`, and a head it was reached through, with the reason `declares`. It SHALL list no sibling of a head, and no constructor as an override.

#### Scenario: A class that overrides the member is listed
- **WHEN** `Sub` extends `Base` and declares `place`, `a.ts` uses `Sub.place`, and impact is asked about `Base.place`
- **THEN** impact lists `sub.ts` with the caller `Sub.place` and the reason `overrides`, and not `a.ts`

#### Scenario: A class that implements the member of an interface is listed
- **WHEN** `Aws` implements `GitProvider` and declares `groups`, and impact is asked about `GitProvider.groups`
- **THEN** impact lists the file of `Aws` with the caller `Aws.groups` and the reason `overrides`

#### Scenario: An override through a chain of classes is listed
- **WHEN** `Sub2` extends `Sub`, which extends `Base` and does not declare `place`, `Sub2` declares `place`, and impact is asked about `Base.place`
- **THEN** impact lists the file of `Sub2` with the reason `overrides`

#### Scenario: The member an override overrides is listed
- **WHEN** `Aws` implements `GitProvider`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact lists the file of `GitProvider` with the reason `declares GitProvider.getGitGroups`, and the note about the head stays

#### Scenario: A sibling implementation is not listed
- **WHEN** `Github` also implements `GitProvider` and declares `getGitGroups`, and impact is asked about `Aws.getGitGroups`
- **THEN** impact does not list the file of `Github`

#### Scenario: A constructor is no override
- **WHEN** `Sub` extends `Base`, both declare `constructor`, and impact is asked about `Base.constructor`
- **THEN** impact does not list `sub.ts`

### Requirement: A load names what it takes
Asked about names, a dynamic import or a `require` that names what it takes SHALL take those names and no others, where it is written. A `require` that binds `default` to a local holds the module: the file uses a name where it uses `local.name`, and a bare use of the local names nothing. One below the top level takes a name only where the file reads it. The answer counts the files left out in a note. A load that takes the module itself SHALL take every name.

#### Scenario: A load takes the names read from it
- **WHEN** `a.ts` declares `foo` and `bar`, `b.ts` has `const { bar } = await import('./a')` in `load`, `c.ts` has `(await import('./a')).foo()` at its top level, `d.ts` has `const lib = await import('./a')` in `start`, and impact is asked about `foo` in `a.ts`
- **THEN** impact lists `c.ts` reached at load and `d.ts` with the caller `start`
- **AND** it does not list `b.ts`

#### Scenario: A require held by a local takes a name where the file uses it
- **WHEN** `lib.js` declares `helper` and `other` and lists them as its exports, `user.js` has `const lib = require('./lib')` at its top level and declares `run`, which uses `lib.helper`, and `stop`, which uses `lib.other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` and not `stop`

#### Scenario: A local a require binds names nothing by itself when passed away
- **WHEN** the files are those of the previous scenario, and `user.js` declares `pass`, which uses `lib`, and `idle`, which uses nothing
- **THEN** impact lists no file
- **AND** a note says that 1 file loads this module and reads none of the names asked about

#### Scenario: A destructured require takes its names
- **WHEN** `user.js` has `const { helper, other: d } = require('./lib')` at its top level and declares `run`, which uses `helper`, and `stop`, which uses `d`, and impact is asked about `helper` and then about `other` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` for `helper`, and with the caller `stop` for `other`

#### Scenario: A require of a file that exports one value reads as that value
- **WHEN** `foo.js` declares `Foo` with the methods `start` and `stop` and exports it as `default`, `user.js` has `const Foo = require('./foo')` at its top level and declares `run`, which uses `Foo.start`, and `halt`, which uses `Foo.stop`, and impact is asked about `Foo.start` in `foo.js`
- **THEN** impact lists `user.js` with the caller `run` and not `halt`

#### Scenario: A default import of a file that exports no default holds the module
- **WHEN** `lib.js` lists `helper` and `other` and no default, `user.js` has `import lib from './lib'` and declares `run`, which uses `lib.helper`, and `stop`, which uses `lib.other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `run` and not `stop`

#### Scenario: A local below the top level takes a name where the file reads it
- **WHEN** `reads.js` and `holds.js` each have `const lib = require('./lib')` inside a function, `reads.js` reads `helper` on a value and `holds.js` reads only `other`, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `reads.js` and not `holds.js`
- **AND** a note says that 1 file loads this module and reads none of the names asked about

#### Scenario: A require used as it stands takes every name
- **WHEN** `user.js` has `require('./lib')` in a function `load` and uses nothing of it, and impact is asked about `helper` in `lib.js`
- **THEN** impact lists `user.js` with the caller `load`

#### Scenario: A bare use of a required module that has a default is no use of its other names
- **WHEN** `reply.js` lists `default` and `buildReply`, and three files have `const Reply = require('./reply')`: `bare.js` uses `Reply` alone, `member.js` uses `Reply.buildReply`, and `named.js` uses `Reply` and reads `buildReply` on a value, and impact is asked about `buildReply`
- **THEN** impact lists `member.js` and `named.js` and not `bare.js`
- **AND** a note says that 1 file loads this module and reads none of the names asked about

### Requirement: Members reached through a field
A use of `Type.member` SHALL refer to that member only, where a use of `Type` alone refers to every member. Asked about a member, `impact` SHALL list the declarations that use it, and those that use its class or an heir of it alone only where their file reads the member by name on some value. Asked about a class, it SHALL list those that use the class or any member of it. A member SHALL go on to the files that import its class, and a namespace import SHALL go on to the member of its class.

#### Scenario: A call through a field names the member
- **WHEN** `service.ts` declares `InvoiceService` with the methods `issue` and `cancel`, `resolver.ts` imports it and declares `InvoiceResolver.issueInvoice`, which uses `InvoiceService.issue`, `InvoiceResolver.cancelInvoice`, which uses `InvoiceService.cancel`, and `InvoiceResolver.all`, which uses `InvoiceService` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts`
- **THEN** impact lists `resolver.ts` with the caller `InvoiceResolver.issueInvoice`
- **AND** it does not list `InvoiceResolver.all` or `InvoiceResolver.cancelInvoice`

#### Scenario: Asked about the class, a call of any member is listed
- **WHEN** the files are those of the previous scenario and impact is asked about `InvoiceService` in `service.ts`
- **THEN** impact lists `resolver.ts` with the callers `InvoiceResolver.all`, `InvoiceResolver.cancelInvoice` and `InvoiceResolver.issueInvoice`

#### Scenario: A member goes on to the files that import its class
- **WHEN** `page.ts` imports `InvoiceResolver` from `resolver.ts` and declares `Page.open`, which uses `InvoiceResolver.issueInvoice`, `Page.close`, which uses `InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `InvoiceResolver` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2 with the caller `Page.open`, and not `Page.all` or `Page.close`

#### Scenario: A member goes on under the name a barrel exports its class by
- **WHEN** `index.ts` re-exports `InvoiceResolver` as `Res` from `resolver.ts`, `page.ts` imports `Res` from `index.ts` and its `Page.open` uses `Res.issueInvoice`, `Page.close` uses `Res.cancelInvoice` and `Page.all` uses `Res` and reads no `issue`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2 with the caller `Page.open`
- **AND** it does not list `index.ts`

#### Scenario: A namespace import goes on to the member of its class
- **WHEN** `page.ts` has `import * as res from './resolver'` and declares `Page.open`, which uses `res.InvoiceResolver.issueInvoice`, `Page.close`, which uses `res.InvoiceResolver.cancelInvoice`, and `Page.all`, which uses `res`, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `page.ts` at distance 2, imports `resolver.ts`, with the caller `Page.open`
- **AND** it does not list `Page.close` or `Page.all`

#### Scenario: A member a subclass inherits reaches the callers of the subclass
- **WHEN** `audit.ts` imports `InvoiceService` from `service.ts` and declares `AuditedService`, which uses `InvoiceService` and whose file reads `issue`, `page.ts` imports `AuditedService` and declares `Page.open`, which uses `AuditedService.issue`, and `Page.idle`, which uses nothing, and impact is asked about `InvoiceService.issue` in `service.ts` with depth 2
- **THEN** impact lists `audit.ts` at distance 1 with the caller `AuditedService`, and `page.ts` at distance 2 with the caller `Page.open` and not `Page.idle`

#### Scenario: A file that holds a class and never names the member is no caller
- **WHEN** `page.ts` declares `hold`, which uses `Service`, reads no `place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists no file
- **AND** a note says that 1 file holds `Service` and names no `place`, and that asking about `Service` lists it

#### Scenario: A file that holds a class and reads the member on any value is a caller
- **WHEN** the files are those of the previous scenario and `page.ts` reads `place` on some value
- **THEN** impact lists `page.ts` at distance 1 with the caller `hold`
- **AND** the answer has no note

#### Scenario: Asked about the class, a holder is listed
- **WHEN** the files are those of the first holder scenario and impact is asked about `Service` in `service.ts`
- **THEN** impact lists `page.ts` at distance 1 with the caller `hold`

#### Scenario: A value of a class passed away is no caller of its members
- **WHEN** `service.ts` declares `Service` with `place` and `shared`, an heir of `Service`, `page.ts` imports `shared` and declares `go`, which uses `shared` and reads no `place`, and impact is asked about `Service.place` in `service.ts`
- **THEN** impact lists no file

#### Scenario: A holder in the file of the class needs the member's name too
- **WHEN** `service.ts` declares `Service` with `place` and `make`, which uses `Service` and reads no `place`, and impact is asked about `Service.place`
- **THEN** impact lists no file

### Requirement: A namespace import names what it reads
In the walk over names, a namespace import SHALL be a caller only of the names it uses, as `lib.helper`, in a declaration and in a load, also through a barrel that exports the namespace. A use of the namespace alone names nothing: the file SHALL be a caller only where it reads the name on some value, and otherwise it is counted in a note and not listed.

#### Scenario: A namespace import is a caller only of the names it uses
- **WHEN** `svc.ts` exports `helper` and `other`, `a.ts` has `import * as lib from './svc'` with `viaHelper` using `lib.helper` and `viaOther` using `lib.other`, `b.ts` has the same import and `onlyOther` using `lib.other`, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists `a.ts` with the caller `viaHelper` and not `viaOther`, and does not list `b.ts`

#### Scenario: A namespace used as a value is held
- **WHEN** `c.ts` and `d.ts` have the same import, `pass` in `c.ts` uses `lib` alone as `consume(lib)` does, `pick` in `d.ts` uses `lib` alone as `lib[key]()` does, and impact is asked about `helper` in `svc.ts`
- **THEN** impact lists no file
- **AND** a note says that 2 files load this module and read none of the names asked about

#### Scenario: A namespace passed away names nothing by itself
- **WHEN** `page.ts` has `import * as lib from './lib'` and declares `go`, which uses `lib` alone, and impact is asked about `helper` in `lib.ts`
- **THEN** impact lists no file

#### Scenario: A file that reads the member and holds the namespace as a value is a caller
- **WHEN** the files are those of the previous scenario and `page.ts` reads `helper` on some value
- **THEN** impact lists `page.ts` with the caller `go`
- **AND** the answer has no note

#### Scenario: A file that holds a namespace as a value is named in a note and not listed
- **WHEN** `page.ts` has `import * as lib from './lib'` and declares `go`, which uses `lib` alone and reads no `helper`, and impact is asked about `helper` in `lib.ts`
- **THEN** impact lists no file
- **AND** a note says that 1 file loads this module and reads none of the names asked about, and that asking about `lib.ts` lists it

#### Scenario: A load reads through a namespace by the names it uses
- **WHEN** the code of `boot.ts` that runs as it loads uses `lib.helper` of the same import, and impact is asked about `helper` and about `other` in `svc.ts`
- **THEN** impact lists `boot.ts` with a load for `helper` and does not list it for `other`

#### Scenario: A name goes on through a namespace a barrel exports
- **WHEN** `barrel.ts` has `export * as ns from './svc'`, `page.ts` imports `ns` from it with `open` using `ns.helper` and `close` using `ns.other`, and impact is asked about `helper` in `svc.ts` with depth 2
- **THEN** impact lists `page.ts` with the caller `open` and not `close`

### Requirement: A type reads no member
A type that names a class SHALL read no member of it. Asked about a member, a file that names the class only in a type, as `import('x').Y` does, SHALL be a holder of the class, and its declarations SHALL be callers only where the file reads that member.

#### Scenario: A type that names a class reads no member of it
- **WHEN** two files name `Runner` in a type and only one of them reads `start` on a value
- **THEN** asked about `Runner.start`, only that one is a caller
- **AND** asked about `Runner`, both are
