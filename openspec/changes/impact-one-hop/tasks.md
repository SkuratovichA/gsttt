# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The walk

- [x] 2.1 `crates/core/src/query/impact.rs`: the depth is one hop through code when the request gives none, for a walk by files and by names; the walk with no depth stays to count and to give each test its distance, reason, callers and load
- [x] 2.2 `crates/core/src/query/impact.rs`: a test is listed only when the walk within the depth reaches it, the rest are counted in `tests_left_out`; with the tests left out all are counted
- [x] 2.3 `crates/core/src/query/impact.rs`: the note that the depth stopped the walk is given for a walk by files too, and the first line no longer ends `; tests at any depth`

## 3. The tests

- [x] 3.1 `crates/core/tests/queries/{impact,names}.rs`: a scenario whose walk is longer than one hop asks with its depth; a test for each new or changed scenario; the property tests take the depth as the bound of the tests
- [x] 3.2 `crates/gsttt/tests/cli/impact.rs`, `crates/mcp/tests/server/answers.rs` and the two server snapshots follow the new default and wording

## 4. The wording

- [x] 4.1 `crates/gsttt/src/cli.rs` and `crates/mcp/src/{arguments,tools,handler}.rs`: the default of one hop, and the depth bounding the tests
- [x] 4.2 `plugin/skills/gsttt/SKILL.md` and `docs/architecture.md`
