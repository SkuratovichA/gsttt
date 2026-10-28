# Proposal

## Why

Every answer gsttt gives is read from an index, and nothing exists yet that builds one. The engine is the piece every connector plugs into and every query reads from, so it comes first and fixes the guarantees the rest relies on: one owner per file, facts reused by content, failures contained, output deterministic.

## What Changes

- `Workspace::locate`, the file listing from git (and from a directory scan outside git) and blob-id content ids.
- File assignment to connector projects, fact keys, reuse from this and sibling worktrees' snapshots, parsing on rayon with size and encoding limits.
- The prepare, resolve, embedded-document and link phases, with a caught panic turned into a status or a coverage line.
- The snapshot file (`GSTTSNAP`, JSON header, rkyv body), `load`, `peek`, freshness, and the no-op refresh.
- Pack merging and coverage counting.

## Capabilities

### New Capabilities

- `indexing`: building an index from a working tree, storing it, and telling how fresh it is.

### Modified Capabilities

None.

## Impact

`crates/core/src/engine.rs` and `crates/core/src/engine/` (new), additions to `index.rs`, `coverage.rs` and `graph.rs`, and engine tests in `crates/core/tests/`. New dependencies are already in the workspace: gix, rayon, rkyv, xxhash-rust, globset, ignore, tempfile.
