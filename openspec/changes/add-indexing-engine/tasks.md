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

## 5. Review fixes

- [x] 5.1 A refresh answers from the snapshot only when the content id of every file and the HEAD commit match too, so an edit to a file no connector reads and a new commit rebuild the index
- [x] 5.2 A file in a merge or rebase conflict stays in the file set, hashed as it is in the work tree
- [x] 5.3 A snapshot that cannot be written is logged and the built index is returned
- [x] 5.4 A HEAD or a history git cannot read becomes a coverage line instead of a silent unborn branch or a dirty count of 0
- [x] 5.5 A panic in `detect` or `matches` is contained (false and a coverage line), and a file is offered only to the projects whose root lies above it
- [x] 5.6 An option of the wrong type fails the run with an error that names the option, as an unknown one already did
- [x] 5.7 A file saved between the listing and the read of a run is filed under the id of the bytes that were read, and one that changed while it was being read has no id
- [x] 5.8 Sibling snapshots are read only when the own snapshot is absent or lacks the facts of more than max(64, 5%) of the claimed files, their headers are checked before their bodies, and a run reports how many it read
- [x] 5.9 The no-op comparison, the stamps of the listing and the seeds read the snapshot where it lies, found by path with a binary search, and copy out only the index and the facts of the files that are taken; a file whose read failed is stored without a key
- [x] 5.10 A file above `max_file_bytes` is not read for its id (its size and modification time stand in), the 256 MiB hashing limit is gone, and only a file that cannot be read has no content id
- [x] 5.11 A stored size and modification time vouches for a file only when the file was written more than two seconds before the listing that took it, and a modified or untracked file in git keeps its stored id the way one outside git does
- [x] 5.12 A snapshot is synced before it is renamed, holds an xxh3 hash of its body in its prefix that opening checks, and names its layout by a number bumped by hand beside the package version
- [x] 5.13 `quiet_contained_panics` installs a panic hook that is silent about the panics the engine contains, on any thread, and passes every other panic to the hook before it
- [x] 5.14 `connector::salt_of` makes a project salt out of text in the framing of the fact keys, the project root is part of every fact key whatever the salt holds, and the `Project::salt` doc says what the salt covers on top of the key
