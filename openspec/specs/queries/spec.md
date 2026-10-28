# queries Specification

## Purpose
Queries read an index and answer what a change affects and how files and contracts connect, as short text for people and agents and as JSON for scripts. Every answer says what it searched, how much of the tree was read and how fresh the index is.

## Requirements

### Requirement: Impact through imports
`impact` SHALL return every file that reaches a changed file through import edges followed backwards to a fixpoint, with its distance in hops and the first reason it was reached. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. A depth limit SHALL stop the walk at that many hops. The changed files themselves SHALL NOT be listed as affected.

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
`dependents` SHALL list the files that import a file and `dependencies` the files it imports, one level, each with the kinds of its edge. `dependencies` SHALL also list the file's imports that resolved to packages, to builtins and to nothing, the last with their reasons.

#### Scenario: Dependencies show every outcome
- **WHEN** `src/a.ts` imports `./b`, `react`, `node:fs` and `./missing`
- **THEN** dependencies lists `src/b.ts` with its edge kind, the package `react`, the builtin `node:fs` and `./missing` with the reason `no such file`

#### Scenario: Dependents show edge kinds
- **WHEN** `b.ts` imports `a.ts` and `c.ts` re-exports everything from it
- **THEN** dependents of `a.ts` lists `b.ts` with a static import and `c.ts` with a re-export of everything

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
