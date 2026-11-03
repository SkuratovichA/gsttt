# Proposal: Follow a method through what a factory gives

## Why

`const s = Service.make(); s.place()`, `Service.getInstance().place()` and `const s = makeService(); s.place()` name no class on the value, so the caller of `Service.place` is missed. A chain such as `new FixTracker(a).retain(b).remove(c)` loses the class after its first call. Against the find-all-references of the TypeScript compiler this is one of the forms of a method call that gsttt still misses.

## What Changes

- TypeScript: a top-level function, or a top-level `const` holding an arrow or a function expression, is an heir of the class it gives: the class of its written return type (`Service`, `Promise<Service>`, `Service | null`), else the class of a `return new X()` of its own body.
- TypeScript: a top-level `const x = f()` of a bare name `f` bound in the file is an heir of `f`, so a value made by a factory and exported reaches its importers.
- TypeScript: a static call on a class, `Service.make()`, names the class of its value: a member read from it is a use of `Service.member` only, and `const s = Service.make()` holds `Service`. A head is an upper-case name bound to an import or a class of the file, never a namespace import.
- TypeScript: a chain on a value of a known class stays on that class, `new FixTracker(a).retain(b).remove(c)`.
- Every new use stays filtered by the member name, so a file that calls a factory and reads no such member is not listed.
- The requirements "Members read through what a factory gives" (`typescript`) and "Callers through what a factory gives" (`queries`) get their scenarios.
- Versions: `READER_VERSION` of the TypeScript connector goes from 29 to 30, because what it stores for a file changes. No change to the core.

## Impact

- Capabilities `typescript` and `queries`.
- `crates/lang-ts/src/analyze.rs`, `crates/lang-ts/src/analyze/roots.rs`, `crates/lang-ts/src/analyze/constants.rs`, `crates/lang-ts/src/analyze/constants/scopes.rs`, `crates/lang-ts/src/analyze/symbols.rs` and `crates/lang-ts/src/project.rs`.
- The snapshots of the TypeScript connector gain heirs for a constant made by a call, such as `app` of `express`.
