# LEARNING

Mistakes made in this repository and what fixed them.

## A fact type changed shape and the snapshot layout did not

`StoredFile` keeps each file's `FileFacts` in the rkyv body of the snapshot, so a new field in `FileFacts`, or in any type it holds, changes the archived layout. Bumping `FACT_FORMAT` in `crates/core/src/engine/keys.rs` makes every cached fact be read again, but it does not stop a snapshot written by the previous build from being opened as the new layout. Fix: bump `LAYOUT` in `crates/core/src/engine/snapshot.rs` in the same commit as any change to a type the snapshot body holds, so that `parse_prefix` refuses the old file.

## Builds in several worktrees at once failed with errors the code did not have

`CARGO_TARGET_DIR` is set in the environment to the `target` of the main checkout, so every worktree builds there and not into its own `target`. Two worktrees with different sources of the same crates rebuilt each other's artifacts and got "unresolved import" errors that neither tree had. Fix: when more than one worktree builds at a time, give each its own with `CARGO_TARGET_DIR=<worktree>/target` on the command.
