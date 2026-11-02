# Proposal: List every test an impact answer reaches, whatever the depth

## Why

A field test on two TypeScript repositories asked `impact` by name about functions whose change broke tests. At the default depth of 1 the answer caught 9 of the 52 breaking tests (recall 0.17, precision 1.00), because the walk by name stops at the direct callers and most breaking tests sit further away: 17 at two hops, 8 at three, 17 at four and 1 at five. With `--depth 100` recall was 1.00 at precision 0.54.

A depth exists to keep the list of files to read short. The tests are what one runs after the change, and a test two callers away runs the change as surely as one at the first, so a depth that cuts the tests answers a question nobody asked.

## What Changes

- The depth of an `impact` answer bounds the affected files and the notes, and never the tests. The tests are every test file that the same walk with no depth reaches, each with that walk's distance, reason, callers and load, and `tests_left_out` counts that walk's tests. This holds for the walk by files and the walk by names alike, so `--depth` means one thing in every impact question.
- `impact` runs two walks when a depth applies: the walk with the depth gives the affected files, which are its hits that are not tests, and the notes; a walk with no depth gives the tests. A walk by name always has a depth, 1 unless the request gives one, so it runs twice; a walk by files runs twice only when the request gives a depth. Without a depth the one walk gives both, as before.
- The walk by name takes an optional depth, as the walk over files does, so that it can run to the end. A walk with no depth has no parts at the depth and never says it stopped.
- The footer covers the changed files and the files of both walks. The possible misses are looked for only when both walks reached nothing, so an answer whose limited walk reached nothing while tests lie past the depth never says that nothing is affected.
- Whenever a depth applies, the first line of the text says `tests at any depth` after it: `impact of foo in a.ts, within 1 hop through code; tests at any depth`. The note that the depth stopped the walk says that only the tests are listed past it.
- A new requirement, `Tests past the depth`, with three scenarios: a test past the depth of a walk by name, a depth on a walk by files that leaves the tests whole, and the tests left out past the depth that are counted. `Impact through imports` says in a clause that a depth stops the walk for every file but the tests, and drops its cross-reference to the requirements on names, which states no rule, to stay within the 500 characters a requirement may hold. Its four scenarios are carried word for word.
- The help of `--depth` and `--name`, the descriptions of `depth` and `names` of the MCP tool, the server instructions, the message of a cut answer, the plugin skill and the architecture document say that the tests are followed to every depth.

## Impact

- Capability `queries`.
- `crates/core/src/query/impact.rs` and `crates/core/src/query/walk/names.rs` (the walk by name takes an `Option<u32>` depth), the wording in `crates/gsttt/src/cli.rs`, `crates/mcp/src/{arguments,tools,handler,ask}.rs`, `plugin/skills/gsttt/SKILL.md` and `docs/architecture.md`, and the tests and snapshots that pin the first line of an answer, the note that the walk stopped, the cut message, the tool texts and the server instructions.
- Cost: a question with a depth walks twice. The second walk has no depth, so on a large repository it can reach every file that depends on the change, and the answer lists the tests among them.
- No version bump: the change is in the queries only, and no fact, reader or snapshot layout changes. `FACT_FORMAT`, `READER_VERSION`, the connector versions and `LAYOUT` stay.
