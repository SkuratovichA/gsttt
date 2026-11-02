# Proposal: Answer --changed by the declarations a patch changes

## Why

`gsttt impact --changed` walks every file git lists whole, so a one-line edit in a file of twenty exports reports the importers of all twenty. The question after a patch is what the patch affects, and the index already knows which declaration uses which, so the answer can follow only the declarations whose text differs.

## What Changes

- Core: `Engine::changed_files` compares each modified file with its old blob (the one of `HEAD`, or of the merge base with `--base`) and gives the declarations whose text differs, and the declarations of the file that use one of them. The comparison reads the declaration lines recorded in the facts, ignores comments, blank lines and indentation, and treats an import that gains, loses or re-points a name as a change of the declarations that use that name.
- A file falls back to being changed whole when the comparison cannot tell: a language that records no declarations, a declaration removed, renamed or moved, a side-effect import or a top-level statement that changes, a new version that does not parse, an old blob that is missing or not text, and a file that is added or untracked.
- Queries: `ImpactRequest::parts` names the declarations of a file, and the walk starts from them. `Impact::parts` says which declarations each file was asked for, and a file where no declaration differs is not walked and the answer says so.
- The CLI and the MCP server hand the declarations to the query. The header names them (`impact of formatUser in src/format.ts`), and the JSON answer has a `parts` list.
- Versions: none move. The facts and the stored index are unchanged.

## Impact

- Capabilities `cli`, `queries` and `mcp`.
- `crates/core/src/engine/declarations.rs`, `changes.rs`, `refresh.rs` and `pipeline.rs`; `Project::code` in `crates/core/src/connector.rs` and its TypeScript implementation; `crates/core/src/query/impact.rs` and `query/walk/names.rs`; the callers in `crates/gsttt` and `crates/mcp`.
- A change to several declarations of one file reports fewer files than before; a change the comparison cannot read reports what it reported before. Known limits: a statement that shares a line with a declaration counts as part of that declaration, and entries of one name widen to the hull of their lines. A hoisted function that moves walks the file whole, as every modified file did before.
