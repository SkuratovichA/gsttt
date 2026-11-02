# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The comparison

- [x] 2.1 `crates/core/src/engine/declarations.rs`: the declarations whose text differs between two versions, with unit tests
- [x] 2.2 `Project::code` and the TypeScript implementation in `crates/lang-ts/src/source.rs` and `project.rs`
- [x] 2.3 `Change::names`, the old blobs in `changes.rs` and the comparison in `refresh.rs` and `pipeline.rs`

## 3. The query

- [x] 3.1 `ImpactRequest::parts`, `Impact::parts` and the header and the line of a file with no changed declaration in `crates/core/src/query/impact.rs`
- [x] 3.2 `reach_changed` in `crates/core/src/query/walk/names.rs`

## 4. The callers

- [x] 4.1 `crates/gsttt/src/commands/impact.rs` and `crates/mcp/src/ask.rs` hand the declarations to the request

## 5. The scenarios

- [x] 5.1 Tests through the binary in `crates/gsttt/tests/cli/impact.rs`
- [x] 5.2 The query test in `crates/core/tests/queries/impact.rs`
- [x] 5.3 The server test in `crates/mcp/tests/server/answers.rs`

## 6. The docs

- [x] 6.1 `docs/architecture.md`, `plugin/skills/gsttt/SKILL.md` and the instructions of the MCP server in `crates/mcp/src/handler.rs`
