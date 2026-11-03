# Proposal: List the callers of what a method overrides or implements

## Why

Asked about a method of a class, `impact` lists the files that hold that class and name the method. It misses every caller that holds the interface or the base class of it, which may dispatch to the method. Against the find-all-references of the TypeScript compiler on a large NestJS repository, recall on methods is 51.9%, recall on direct references 97.1%, and of the 187 files that are only related, 60 name a head of the class and the member. Also, a member of an interface cannot be asked about: `impact --name GitProvider.getGitGroups` answers that the file declares no such name.

## What Changes

- TypeScript: each member of an interface that has a name written in the source (a method signature or a property signature) is a declaration `Interface.member`, over the lines of its signature, as the member of a class is. Overloads widen one declaration. A type alias of an object type stays one declaration.
- Core: `Symbols.overrides` is a new fact, a list of `Heir { name, of }`: the heads a value of which may run the members of a name. It is kept apart from `heirs`, which means "takes the members of": a Go embedding takes members and no call on the embedded type reaches the outer type. The graph keeps it beside the heirs, and `FileSymbols::overrides` reads it.
- TypeScript: a class records its `extends` head and each type it `implements` in `overrides`. The other connectors record none.
- Core: asked about a member `Class.member`, and from a changed method, the walk starts also from the same member of each head of the class, in the file that declares the head, and of their heads in turn. A head is found through the imports of the file, a barrel, a named re-export, `export *` and `import * as lib`, with `import type`. A head outside the repository, or one that does not declare the member, gives nothing. A holder counts under the existing rule: its file names the member. A sibling implementation declares the member and does not read it, so it is no caller. A note says which member may run through which head.
- The requirement "Declarations" is modified and "Overrides" added (`typescript`), and "Callers through a head" and "Members of an interface can be asked about" (`queries`) get their scenarios.
- Versions: `FACT_FORMAT` goes from 17 to 18, because the facts of an unchanged file would keep no overrides. `LAYOUT` of the snapshot goes from 11 to 12, because the stored table of symbols gains the overrides. `READER_VERSION` of the TypeScript connector goes from 26 to 27, because what it stores for a file changes.

## Impact

- Capabilities `typescript` and `queries`.
- `Symbols` in `crates/core/src/fact.rs`, `crates/core/src/graph/symbols.rs`, `crates/core/src/testing.rs`, `crates/core/src/query/walk/names.rs`, the new `crates/core/src/query/walk/overrides.rs`, `crates/core/src/query/impact.rs`, the recorder in `crates/lang-ts/src/analyze/symbols.rs`, and `crates/lang-go/src/parse.rs`.
- A tree with no interface and no class that implements or extends another gives the same answers as before. The snapshots of the TypeScript connector gain the members of interfaces and an `overrides` list.
