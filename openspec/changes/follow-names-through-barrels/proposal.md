# Proposal

## Why

`impact` treats a file as one unit, so a change behind a barrel, an `index.ts` made of `export *` lines, reaches every importer of the barrel whatever it takes from it. In a TypeScript monorepo of about 3,000 files, a change to one GraphQL resolver reached 259 files through a client barrel of 30 `export *` lines, and about three of them use what changed. An agent that gets such an answer reads hundreds of files it does not need, which is the cost gsttt exists to cut. The edges are right; what the core lacks is which names each importer takes and which names each re-export passes on, though the TypeScript connector already reads both.

## What Changes

- Facts: a file's own exports, or that they are unknown, and the name each named re-export exports its names as. The fact format moves to 2.
- Graph: each import edge carries what the importer uses, what it passes on and whether it re-exports everything, and each file its own exports. The snapshot layout moves to 5.
- `impact` follows a change name by name: through a barrel it reaches only the importers that take a name the change reaches, and wherever the names cannot tell it falls back to the whole file, as today.
- Reasons name what carries the change: `imports useFoo, useBar from src/a.ts`, `re-exports fromA from src/a.ts`.
- `dependents` and `dependencies` show the names each importer takes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `queries`: impact by names, reasons and neighbours with names.
- `typescript`: the connector records exports.

## Impact

`crates/core` (facts, graph, build, snapshot, the walk, impact, neighbours, the testing tree) and `crates/lang-ts` (the analyzer and its version), and `docs/architecture.md`. Every index is rebuilt once, because the fact format and the snapshot layout change. An edge into a file whose exports are unknown is followed at file level, whatever names it carries, and no connector but TypeScript lists exports, so Python and Rust give the same files, distances and reasons as before. Rust now records the name that an aliased `use` takes from the module, and the name it binds apart, so its neighbours show the first; its connector version is 2.
