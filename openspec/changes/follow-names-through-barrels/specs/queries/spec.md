# Spec Delta

## MODIFIED Requirements

### Requirement: Impact through imports
`impact` SHALL return every file that reaches a changed file through import edges followed backwards to a fixpoint, with its distance in hops and the first reason it was reached, and where the edges name what the importer takes, SHALL carry the change only as far as the names allow, by the requirements on names below. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. A depth limit SHALL stop the walk at that many hops. The changed files themselves SHALL NOT be listed as affected.

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

## ADDED Requirements

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
An edge into a file whose exports are unknown SHALL be followed at file level, whatever names it carries: its importer SHALL be affected in all its names when the file is affected in all of them, with the reason that it imports the file and no names, whether it imports by name, passes names on or re-exports everything. The exports of a file are unknown when its connector lists none, and when it re-exports everything from a file whose exports are unknown. A connector that lists no exports SHALL therefore give the files, distances and reasons of a walk over files.

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
