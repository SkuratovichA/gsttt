# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The walk by name

- [x] 2.1 `crates/core/src/query/walk/names.rs`: `reach_names` and the walk take an `Option<u32>` depth; a walk with no depth has no parts at the depth and never says it stopped

## 3. The answer

- [x] 3.1 `crates/core/src/query/impact.rs`: one helper walks by files or by names; when a depth applies a second walk with no depth gives the tests and the count of tests left out; one place builds the entry of a hit; the misses are looked for when both walks reached nothing; the footer covers both walks
- [x] 3.2 `crates/core/src/query/impact.rs`: the first line ends `; tests at any depth` when a depth applies, and the note that the depth stopped the walk says that only the tests are listed past it

## 4. The wording

- [x] 4.1 `crates/core/src/query/impact.rs`, `crates/gsttt/src/cli.rs` and `crates/mcp/src/{arguments,tools,handler,ask}.rs`: the depth bounds the affected files and not the tests
- [x] 4.2 `plugin/skills/gsttt/SKILL.md` and `docs/architecture.md`

## 5. The tests

- [x] 5.1 One test per scenario of `Tests past the depth`, in `crates/core/tests/queries/names.rs` and `crates/core/tests/queries/impact.rs`
- [x] 5.2 A property over generated trees: by files and by names, at every depth the tests, their rows and the count of tests left out are the same, and a larger depth drops no affected file
- [x] 5.3 The tests that pin the first line, the note that the walk stopped, the cut message, the tool texts and the server instructions follow the new wording
