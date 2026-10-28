# Design

## Context

`impact` walks the reverse import graph file by file (`crates/core/src/query/walk.rs`). A barrel with thirty `export *` lines makes every importer of the barrel an importer of each of the thirty files, so one change behind it reaches everything that imports anything from the barrel. The TypeScript connector already reads which names each import takes (`Import.names`) and what each file exports (`TsExtra.exports`), but the core sees only file-to-file edges with kinds and flags (`Edge` in `crates/core/src/graph.rs`).

## Goals

- A change behind a barrel reaches the importers that take a name it reaches, and no others.
- The answer says which names carry the change, so that an agent reads the right lines instead of whole files.
- Connectors that record no names, and imports without names, give exactly the answers they give today.
- No file the change can break is dropped: wherever the names cannot tell, the whole file is affected.

## Non-goals

- Following member access on a namespace import (`api.foo`): a namespace import takes every name.
- Edges between symbols inside a file: a file that uses an affected name is affected in all its names.

## The data

- Facts: `FileFacts.exports: Option<Vec<String>>` holds the names a file exports itself, sorted, and `None` when they are unknown; `Import.exported: Vec<String>` holds, for a named re-export, the name each entry of `names` is exported as. Both are left out of the JSON when empty, the way `TsExtra` does it; the cached bytes are rkyv, which carries every field, so `FACT_FORMAT` 2 is what keeps an old cache from being read.
- Graph: names are interned once, in byte order. Each edge has a binding: `uses`, which is all or a sorted set of names that may be empty; `passes`, sorted pairs of the name exported as and the source name; and `star`, whether the edge re-exports everything. Each file has its own exports, or unknown. A binding merges with its edge: `uses` by union with all absorbing every set, `passes` by union, `star` by or.
- The binding of one import: a named re-export passes its names under their aliases and uses nothing; `export * as ns` passes `ns` for `*`; `export *` sets `star` and uses nothing; an import whose names are known and hold no `*` uses those names; every other import uses all, which is what every import of a connector that records no names becomes.

## The walk

Each reached file F holds A(F), the names of F the change may reach, or all. The changed files and every file reached across a contract start with all. The walk keeps today's layers: a frontier per distance, in file id order, where each entry carries Δ, the names that are new to that file. Over the edge of an importer I into a file T:

- `uses` all: I gets all when Δ is not empty.
- `uses` U, not empty: I gets all when Δ is all, when U and Δ share a name, or when U holds a name that is not in X(T) or X(T) is unknown.
- `passes` (as, src): I gets `as` when Δ is all, when src is in Δ, or when src is `*` and Δ is not empty.
- `star`: I gets Δ without `default`, and when Δ is all it gets X(T) without `default`, or all when X(T) is unknown.
- When the own exports of I are unknown and I gets anything, I gets all.

X(T), all the names of T, is the own exports of T together with X(U) without `default` for every `star` edge from T to U, and it is unknown when any of those is. It is computed when first needed and kept, and closed over cycles of `export *`: a cycle shares one set. A file whose names grow goes into the next frontier with only its growth, and since the sets only grow the walk ends. A file keeps the distance and the reason of its first visit, the tie rules stay as they are, and within one edge a use comes before a pass. Only a file whose names become all crosses its contracts, at the moment they do.

## Reasons

`Reason::Imports { file, names }` and a new `Reason::ReExports { file, names }`, written `via: "re-exports"` in JSON, with the names that carried the change, sorted, and none when the edge uses all. Text: `imports useFoo, useBar from src/a.ts`, `re-exports fromA from src/a.ts`, and `imports src/a.ts` with no names; the first three names and then `and N more`, unless every entry is asked for.

## Decisions

- A name the target does not export is affected. A change that removes an export behind `export *` leaves no trace in the new facts but the name in the importer, and the importer is exactly what the change breaks.
- Unknown exports fall back to the whole file, so CommonJS, `export =` and files read past errors stay as safe as they are today.
- Only whole files cross contracts: a barrel reached for some of its names has not changed what it does.
- The binding lives in the core and knows no language. TypeScript is the only connector that records exports today.

## Rejected

- A graph of symbols instead of files: a change to every query for little more than this gives.
- Cutting the answer by depth: it drops real hits as readily as noise.
