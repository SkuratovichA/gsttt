# Spec Delta

## MODIFIED Requirements

### Requirement: Impact through imports
`impact` SHALL return every file that reaches a changed file through import edges followed backwards to a fixpoint, with its distance in hops and the first reason it was reached; where the edges name what the importer takes, the change SHALL go only as far as the names allow. An edge whose only kind is a Rust `mod` declaration SHALL NOT carry impact. A depth SHALL stop the walk at that many hops through code for every file but the tests. The changed files SHALL NOT be listed as affected.

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

## ADDED Requirements

### Requirement: Tests past the depth
The depth of an `impact` answer SHALL bound the affected files and SHALL NOT bound the tests, for the walk by files and the walk by names alike: the tests SHALL be every test file the same walk with no depth reaches, each with the distance, the reason, the callers and the load of that walk, and the tests left out SHALL be counted from that walk. Whenever a depth applies, the first line of the text SHALL say `tests at any depth` after it.

#### Scenario: A test past the depth of a walk by name is listed
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, `c.ts` imports `run` from `b.ts` and uses it in `main`, `c.test.ts` imports `main` from `c.ts` and calls it in a callback of `test`, and impact is asked about `foo` in `a.ts` without a depth
- **THEN** impact lists `b.ts` and not `c.ts`, and lists `c.test.ts` among the tests at distance 3, reached at load
- **AND** the text starts with `impact of foo in a.ts, within 1 hop through code; tests at any depth`

#### Scenario: A depth on a walk by files leaves the tests whole
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, `c.test.ts` imports `c.ts`, and `a.ts` changes with depth 1
- **THEN** impact lists `b.ts` and not `c.ts`, and lists `c.test.ts` among the tests at distance 3 with the reason `imports c.ts`

#### Scenario: Tests left out past the depth are counted
- **WHEN** `c.ts` imports `b.ts`, `b.ts` imports `a.ts`, `c.test.ts` imports `c.ts`, `a.ts` changes with depth 1, and impact is asked to leave the tests out
- **THEN** impact lists `b.ts` and lists no tests
- **AND** it says it left out 1 test file
