# Proposal: Give a star barrel no row when a name is asked about

## Why

An agent asks what one function affects, and every extra row costs it tokens. In the walk over names an `export *` barrel gets a row of its own, `re-exports helper from a.ts`, though nothing in that file names `helper`: changing `helper`, even renaming it, needs no edit there and its behaviour does not depend on it. Against the find-all-references of the TypeScript compiler on a real repository, these rows are 37 of the 584 extra files. A named re-export, `export { helper } from './a'`, does name it, and the compiler counts it, so it keeps its row.

## What Changes

- Core: in the walk over names, asked about a name, a file that passes it on by `export *` gets no row. The walk still goes through it with no hop, and its importers are listed as before. A file reached whole keeps its row.
- The walk over files, `path` and `dependents` do not change, and neither do the requirements "Names through re-exports" and "Star re-exports keep their importers".
- Docs: `docs/architecture.md` says that an `export *` gets no row in the walk over names.
- No fact changes, so `READER_VERSION`, `FACT_FORMAT` and `LAYOUT` stay.

## Impact

- Capability `queries`.
- `by_name` in `crates/core/src/query/walk/names.rs`.
- An answer about a name lists fewer files where a star barrel stood between the name and its importers.
