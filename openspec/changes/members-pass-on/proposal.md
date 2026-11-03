# Proposal: Pass a member on to what takes it

## Why

Against the find-all-references of the TypeScript compiler on Amplication, gsttt finds 71% of the direct callers of a method. 54 of the 61 it misses call a method that a subclass inherits: `ModuleMap extends FileMap`, and `new ModuleMap().set(file)` elsewhere is a use of `ModuleMap.set`, which nothing links to `FileMap.set`. A second form is `export const orders = new OrderService()` with `orders.place()` in an importer, which records `orders.place` and nothing links it to `OrderService.place`.

## What Changes

- Core: `Symbols.heirs` is a new fact, a list of `Heir { name, of }`: a name that takes the members of the class or interface `of`. The graph keeps it in the flat table of symbols beside the exports, and `FileSymbols::heirs` reads it.
- Core: asked about a member, the walk goes on, in the same hop, to each name that takes the members of its class and does not declare that member, through the heirs of the heirs. A private member (`#x`) does not pass on. An importer of such a name is a caller when it uses `Heir.member`, or the heir alone, and the heir is no hop of its own. In the file of the member, a declaration that uses `Heir.member` is a caller. Through a namespace import the head is matched as `lib.Base`. Asked about a class as a whole, the walk is unchanged.
- TypeScript: a class records the head of its `extends`, an interface each head of its `extends`, a top-level `const` the class it holds by `new` or by a plain type, and `export default new Service()` records `default`. `implements`, `let`, `var` and a value below the top level record nothing.
- TypeScript: a member read from a name with a heir pair is used by its name, `this.member` of a member the class does not declare is a use of `Head.member`, `super.member` is always a use of `Head.member`, and `super(...)` a use of `Head.constructor`.
- The requirements "Heirs" (`typescript`) and "Members that pass on" (`queries`) get their scenarios.
- Versions: `LAYOUT` of the snapshot goes from 8 to 9, because the stored table of symbols gains the heirs. `FACT_FORMAT` goes from 14 to 15, because the facts of an unchanged file would keep no heirs. `READER_VERSION` of the TypeScript connector goes from 19 to 20, because what it stores for a file entry changes.

## Impact

- Capabilities `typescript` and `queries`.
- `Symbols` in `crates/core/src/fact.rs`, `crates/core/src/graph/symbols.rs`, `crates/core/src/query/walk/names.rs`, `crates/core/src/testing.rs`, the recorder in `crates/lang-ts/src/analyze/symbols.rs` and `crates/lang-ts/src/analyze.rs`, and `docs/architecture.md`.
- A tree with no class that extends another and no top-level instance gives the same answers as before. The snapshots of the TypeScript connector gain a `heirs` list.
