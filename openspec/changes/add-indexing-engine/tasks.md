# Tasks

## 1. Workspace and files

- [x] 1.1 `Workspace::locate` for main worktrees, linked worktrees and plain directories; bare repositories are an error
- [x] 1.2 File listing from git (tracked present files, untracked unignored files, minus submodules, symbolic links, skip-worktree entries and `Setup.exclude`)
- [x] 1.3 Content ids: index oids for clean files, blob hashing for modified and untracked ones, the size and mtime shortcut outside git

## 2. A run

- [x] 2.1 Opening connectors in registry order with merged packs; options errors fail the run, project errors become coverage lines
- [x] 2.2 File assignment by registry order and deepest root; unclaimed files counted
- [x] 2.3 Fact keys and reuse from this and sibling worktrees' snapshots; parsing on rayon with size and UTF-8 limits
- [x] 2.4 Prepare, resolve, embedded documents, link; edges, sites, tags and coverage into `GraphBuilder` and `Coverage`
- [x] 2.5 Panic containment around every connector call

## 3. Snapshot

- [x] 3.1 Snapshot format: magic, format version, JSON header, rkyv body; atomic write; validation on read
- [x] 3.2 `load`, `peek`, freshness, and the no-op refresh
- [x] 3.3 Determinism test across thread counts

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("indexing", …)]` for every scenario of the `indexing` spec
- [x] 4.2 `cargo nextest run -p gsttt-core`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
