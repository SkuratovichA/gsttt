# Proposal: Compare a changed Go file by declaration

## Why

`gsttt impact --changed` walks a modified file by the declarations whose text changed on TypeScript and Python. A Go file gives no code to compare, so it is taken whole, and a one-line edit in a function lists every caller of every name in the file. An answer that overreaches costs the reader tokens.

## What Changes

- A Go file has a code for `--changed`: its text with the comments and the import declarations blanked, and every run of spaces and tabs inside a line squeezed to one space, because gofmt realigns a whole block when one line of it grows. The whitespace inside a string, a raw string or a rune stays part of it, and so do the `//go:` directives, the `// +build` lines and the comments above `import "C"`, which are program. A file with a syntax error, and `go.mod`, have none.
- The requirement that names TypeScript for `--changed` by declaration says every language that gives its code, and `docs/architecture.md` says the same. The Python change left both as they were.
- Not in this change: a shift of `iota` (a constant added above others changes their values and not their text), and a function moved within the file, which reads as a change of order and takes the file whole although the order of the top level means nothing in Go.
- Versions: none. The facts of a Go file do not change, so no connector, reader or fact format version moves.

## Impact

- Capability `go`: "The code of a Go file" is new.
- Capability `cli`: "Changed declarations are walked" is modified to name every language that gives its code, and a changed Go function reaching its callers, a comment-only edit, a field added to a struct and a changed `init` are new.
- `crates/lang-go/src/code.rs`, `crates/lang-go/src/project.rs`, `docs/architecture.md`.
- A Go repository answers `--changed` with the callers of the changed declarations, and not of the whole file.
