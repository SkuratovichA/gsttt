# LEARNING

Mistakes made in this repository and what fixed them.

## A fact type changed shape and the snapshot layout did not

`StoredFile` keeps each file's `FileFacts` in the rkyv body of the snapshot, so a new field in `FileFacts`, or in any type it holds, changes the archived layout. Bumping `FACT_FORMAT` in `crates/core/src/engine/keys.rs` makes every cached fact be read again, but it does not stop a snapshot written by the previous build from being opened as the new layout. Fix: bump `LAYOUT` in `crates/core/src/engine/snapshot.rs` in the same commit as any change to a type the snapshot body holds, so that `parse_prefix` refuses the old file.

## Builds in several worktrees at once failed with errors the code did not have

`CARGO_TARGET_DIR` is set in the environment to the `target` of the main checkout, so every worktree builds there and not into its own `target`. Two worktrees with different sources of the same crates rebuilt each other's artifacts and got "unresolved import" errors that neither tree had. Fix: when more than one worktree builds at a time, give each its own with `CARGO_TARGET_DIR=<worktree>/target` on the command.

## gsttt use in agent transcripts was counted too high

Grepping a transcript of `cargo xtask agents run` for `gsttt` counted every Bash command that names a path in the checkout the agent works in, because that path holds the word. Fix: count the calls of the gsttt MCP tools by their names, and a Bash command only when the program it runs is `gsttt`.

## openspec validate --strict failed on a requirement that had grown

A sentence added to a requirement of a delta took its text past 500 characters, and `openspec validate --strict` turns that warning into a failure. Fix: a new behaviour gets a requirement of its own with its scenario, rather than one more sentence in a requirement that already holds several.
