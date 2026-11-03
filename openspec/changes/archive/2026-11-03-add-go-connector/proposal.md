# Proposal

## Why

Go is a common language for services, and a repository that serves a route from Go and calls it from TypeScript, or the other way round, loses the link between them without a Go connector. Go also differs from the languages gsttt reads so far: its imports name packages, not files, and a package is every file of one directory, so the connector has to decide which files of a package an import reaches.

## What Changes

- The `go` connector: one project per `go.mod`, parsing with `tree-sitter-go`, imports resolved against the module path, the required modules and the standard library.
- Files of one package reach each other by the names they read, so a change to one file reaches the siblings that read what it declares.
- Declarations (functions, methods, types, fields, constants, variables) with what each one uses, so that `impact --name` lists callers of a function or of a method.
- Built-in packs in `crates/lang-go/packs/`: `env` (`os.Getenv` and its relatives), `net-http` (clients and servers), `database-sql` (SQL text passed to `Query`, `Exec` and their relatives).
- The registry and its lists name `go` after `rust`.

## Capabilities

### New Capabilities

- `go`: what the Go connector reads, how it resolves imports, and which contracts it finds.

### Modified Capabilities

- `cli`: the registry registers `go` after `rust`.
- `queries`: Go declarations answer `impact --name`.

## Impact

`crates/lang-go/` is new, with `tree-sitter` and `tree-sitter-go` in the root `[workspace.dependencies]`. `crates/gsttt` registers the connector. `RepoView` in `crates/core` gains `read_regular_file`, which the Go and Python connectors share. No other change to `crates/core` or to another language connector.

### Known gaps

- A method called on a value whose type the connector cannot see is not recorded, so a file that holds only such a caller is not listed.
- A conversion `T(x)` and an argument `new(T)` name the type as a bare identifier, so they count as a use of every member of `T`.
- A call through an interface is not followed to the types that implement it.
- Methods promoted from an embedded type are not recorded as methods of the outer type.
- Build tags and `GOOS` suffixes are ignored: every file of a package counts.
- Routers (chi, gin, echo), receivers of clients (`client := &http.Client{}; client.Get(url)`), gRPC and `link` are not read.
- A value defined in another file is not followed.
- `replace` directives and `go.work` files are not read.
- A file that uses the Go 1.26 form `new(expr)` is read as recovered, because `tree-sitter-go` does not know it yet: it keeps its declarations, and some uses inside that function may be lost.
- A `go.mod` with no `module` line gets an empty module path and a logged warning, since the connector contract has no other channel for a problem at open.
