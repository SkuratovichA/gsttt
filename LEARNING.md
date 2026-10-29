# LEARNING

Mistakes made in this repository and what fixed them.

## A fact type changed shape and the snapshot layout did not

`StoredFile` keeps each file's `FileFacts` in the rkyv body of the snapshot, so a new field in `FileFacts`, or in any type it holds, changes the archived layout. Bumping `FACT_FORMAT` in `crates/core/src/engine/keys.rs` makes every cached fact be read again, but it does not stop a snapshot written by the previous build from being opened as the new layout. Fix: bump `LAYOUT` in `crates/core/src/engine/snapshot.rs` in the same commit as any change to a type the snapshot body holds, so that `parse_prefix` refuses the old file.
