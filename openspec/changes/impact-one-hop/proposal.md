## Why

`impact` followed callers to the end by default, so one changed function could list most of a repository, and a test far from the change was listed beside the nearest ones. One hop answers the usual question, who uses this directly, and asking again about a caller goes further.

## What Changes

- `impact` takes one hop through code by default, for a walk by files and a walk by names alike. A large depth such as 4294967295 runs the walk to the end. A hop across a contract still does not count.
- The depth bounds the tests as it bounds the other files. A test past the depth is counted among the test files left out and not listed. With the tests left out, all of them are counted.
- When the depth stops a walk with more to follow, the answer says so for a walk by files too.
- Scenarios whose walk is longer than one hop now state the depth they need.

No new flag, no new JSON field, and no change to the reason, the row format, the walk rules or the coverage footer.

## Capabilities

### Modified Capabilities

- `queries`: the default depth of `impact`, the depth bounding the tests, the note when the depth stops a walk.

## Impact

- Code: `crates/core` (`query/impact.rs`), and the help and description texts in `crates/gsttt` (`--depth`) and `crates/mcp` (the `depth` argument, the tool description, the server instructions).
- Docs: `docs/architecture.md`, `plugin/skills/gsttt/SKILL.md`, and the `--depth` and `depth` sentences of the `cli` and `mcp` specs.
- No version bump: this changes a query only. `FACT_FORMAT`, `READER_VERSION`, the connector versions and `LAYOUT` stay.
