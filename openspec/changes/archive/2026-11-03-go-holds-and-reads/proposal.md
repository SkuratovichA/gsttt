# Proposal: Follow a Go method through what a function returns, embeds or holds

## Why

Asked about a Go method, gsttt misses most files that call it directly: recall on direct calls is 58.6% on chi, gin, cobra and client_golang, against 97.1% for TypeScript. The misses are a local assigned from a call of a function of another package (`r := chi.NewRouter(); r.Get()`), a range variable, a call result, a method promoted through an embedded field, a struct field and a package-level variable. The walk in core already passes a member on to a name that takes the members of another (`Symbols.heirs`) and counts a use of a type alone only for a file that reads the member, but the Go connector records no heirs and spells a use of a type `Type.`, so neither rule can meet it.

## What Changes

- Go: a type is used by its bare name, `Type`, in every position, and no longer as `Type.`. A file that names `Mux` and reads `Get` counts for `Mux.Get`.
- Go: a call gives no holding but `new(T)`. The name of a constructor is no longer turned into a type: `x := pkg.NewT(); x.M()` uses `pkg.NewT` and reads `M`.
- Go: the connector records heirs. A function takes the members of its first result, a struct and an interface those of each embedded type, an alias those of its type, and a package-level `var` those of the type or the function result it holds.
- The requirement "What a declaration uses" (`go`) says a type is used by its name, drops the constructor forms from its scenario of a method read through a parameter, a local and a range variable, and says that a method read through a constructor is read by name. The requirement "What takes the members of a type" (`go`) and "A method through what a function returns" (`queries`) get their scenarios.
- Versions: `READER_VERSION` of the Go connector goes from 2 to 3, because what it stores for a file changes. `FACT_FORMAT` and `LAYOUT` stay, since `Symbols.heirs` exists in both.

## Impact

- Capabilities `go` and `queries`.
- `crates/lang-go/src/walk.rs`, `analyze.rs`, `parse.rs` and `project.rs`, and the tests of `crates/lang-go` and `crates/gsttt`.
- Every Go snapshot changes: `Type.` becomes `Type`.
