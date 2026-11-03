# Proposal: A holder of a class is a caller only where its file names the member

## Why

Asked about a method `Class.member`, the walk lists every file that holds the class: a Nest module that only lists the service in `providers`, a constant that holds an instance, a subclass that never calls the method. A call through a value names no class, so the walk could not tell these from callers. On Amplication, 80 methods list 625 files that only hold the class, and a grep-level simulation that keeps the files naming the method on any value keeps all 205 true hits.

## What Changes

- Core: `Symbols.reads` is a new fact, the sorted list of member names the file reads on a value of a class it does not know. The graph keeps it in the flat table of symbols, and `FileSymbols::reads_member` asks it.
- Core: asked about `Class.member`, a use of the class alone, or of a name that takes its members (an heir), refers only when the file reads `member`. A use of `Class.member` always refers. Asked about the class as a whole, or about a top-level name, the walk is unchanged. A namespace import used alone stays a use of every name; `lib.Base` alone needs the member name, and `lib.Base.other` is no use of `Base.place`. `--changed` follows the same rule.
- Core: the note "callers of `X` through a value are found through the declarations that refer to `X`" is replaced by a count: `3 files hold Runner and name no start; ask about Runner to list them`, and for several members `3 files hold the classes of Runner.start and Runner.stop and name none of them; ask about those classes to list them`. A file counts when the rule alone left it out; the note is absent when the count is zero.
- TypeScript: a file records the member names it reads on a value of unknown class: `x.m`, `x?.m`, `x['m']`, each name of a chain after the first, `<x.m />`, the keys of an object pattern in a declaration, a parameter, a loop head, a `catch` and an assignment target, and the string a call of `spyOn`, `stub` or `spy` takes second. `this.m`, `super.m`, a read of a known class, an object literal key and a computed `x[k]` record nothing. `T['m']` as a type is a use of `T.m`.
- Core: a re-export of a class gets no row when a member of it is asked about, and `export * as ns` gets no row for a name the namespace holds. The requirement "A star barrel names nothing" (`queries`) is modified with two scenarios.
- Go: a file records the member names it reads on a value whose type it does not know, as the TypeScript connector does, so that the rule above does not drop the callers of a method in Go. A package name, a value of a known type and a struct literal key record none. The requirement "Reads of members" (`go`) is new.
- The requirements "Members called through a field" and "Members of a class" (`queries`) get new and changed scenarios, and "Reads of members" (`typescript`) is new.
- Versions: `LAYOUT` of the snapshot goes from 10 to 11, because the stored table of symbols gains the reads. `FACT_FORMAT` goes from 16 to 17, because the facts of an unchanged file would keep no reads. `READER_VERSION` of the TypeScript connector goes from 20 to 21, because what it stores for a file entry changes. `READER_VERSION` of the Go connector goes from 1 to 2, because a file now stores its reads.

## Impact

- Capabilities `typescript`, `go` and `queries`.
- `Symbols` in `crates/core/src/fact.rs`, `crates/core/src/graph/symbols.rs`, `crates/core/src/query/walk/names.rs`, `crates/core/src/query/impact.rs`, `crates/core/src/testing.rs`, the recorder in `crates/lang-ts/src/analyze/symbols.rs` and `crates/lang-ts/src/analyze.rs`, `docs/architecture.md` and `plugin/skills/gsttt/SKILL.md`.
- A holder of a class that never names the member is no longer a caller of that member. The snapshots of the TypeScript connector gain a `reads` list.
