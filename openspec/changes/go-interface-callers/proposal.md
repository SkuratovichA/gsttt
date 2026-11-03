# Proposal: List the callers of a method through the interfaces its type satisfies

## Why

In Go about half of the references to a method go through an interface, and a Go type never names the interface it satisfies. Against the compiler's references, recall of `impact` on Go methods is 49.3%, against 95.1% on direct references. A call `s.Get("x")` on a value of the interface `Store` is a use of `Store.Get`, and the method `PG.Get` that runs there is not listed as reached by it.

## What Changes

- Core: a connector's link may return heads found across files. `Linked.overrides` carries `LinkedOverride { file, name, owner, head }`, which says that the values of `name`, declared in `file`, may run the members of `head`, declared in `owner`. The engine sorts them and keeps them in a per-file column of the symbols table. `overridden` follows them like the heads a file writes, and `overriding` lists the names that take a head through them.
- Go: the parse keeps, for each file, the methods by receiver with their signatures, the interfaces with their methods and embedded interfaces, the embedded types of structs, and the generic types. The link finds, within a package and across the packages of the module, the types whose method set holds every method of an interface in the same shape, and returns one head for each file of the type that declares it, an embedding in it or a method of it.
- Go: a package is a directory and the name in the package clause, so `foo` and `foo_test` are two. An own method hides a promoted one. An interface with a union or a constraint term, and one that lists no method, are no head. A type that shares only a method name with an interface is no pair. An unexported method ties the pair to one package. Where a type or an interface has type parameters, only the counts of parameters and results are compared.
- Not changed: the link reads one module at a time, so a type does not satisfy an interface of another module (`links_across_projects` stays false). Pointer and value receivers are not told apart.
- Versions: `FACT_FORMAT` goes from 19 to 20, because the same text now gives heads in the graph. `LAYOUT` of the snapshot goes from 12 to 13, because `Span` and the symbols table change shape. `READER_VERSION` of the Go connector goes from 4 to 5, because the data it stores for a file changes.

## Impact

- Capabilities `go` and `queries`.
- `Linked`, `LinkedOverride` in `crates/core/src/connector.rs`, `Output::overrides_of` in `crates/core/src/engine/pipeline.rs`, the symbols table in `crates/core/src/graph/symbols.rs`, `overridden` and `overriding` in `crates/core/src/query/walk/overrides.rs`.
- `GoExtra`, `normal_type` and the link in `crates/lang-go/src/interfaces.rs` and `crates/lang-go/src/interfaces/link.rs`, the reading of methods and interfaces in `crates/lang-go/src/analyze.rs`.
- The README names the gap that is left, and `docs/architecture.md` the new heads.
