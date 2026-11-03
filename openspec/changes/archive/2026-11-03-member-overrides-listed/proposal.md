# Proposal: List what overrides a member and what it overrides

## Why

Asked about a method, the compiler's find-all-references lists the classes that override or implement it and the declaration it overrides. gsttt lists neither, and they are what has to change along with a signature. On the TypeScript field 66 of the 127 truth files gsttt misses outside direct references are such declarations, and 22 of 40 on undici. Three overrides of `Dispatcher.close` were listed only by accident, and are lost since a class stopped reading its members through the class name.

## What Changes

- Core: a reached member `Base.m` lists, in its own file, the same member of each class that extends or implements `Base`, at any depth, with the new reason `overrides`. Its callers are one more hop, as for any reached part. A constructor is no override.
- Core: a head the walk started from because the member asked about overrides it is listed with the reason `declares`, and its siblings stay out.
- Not changed: a removed member lists nothing, because it has no declaration to start from.
- Versions: none move. No fact changes, and an answer gains a reason, which is an addition.

## Impact

- Capability `queries`.
- `Cause::Overrides` in `crates/core/src/query/walk.rs`, `Reason::Overrides` in `crates/core/src/query/impact.rs`, `overriding` in `crates/core/src/query/walk/overrides.rs`, `Step::overriders` in `crates/core/src/query/walk/names.rs`.
