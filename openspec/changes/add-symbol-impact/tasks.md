# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `design.md`, `tasks.md` and the deltas of the `queries`, `typescript`, `mcp` and `cli` specs

## 2. The facts

- [x] 2.1 `Symbols`, `Declared`, `LocalExport` and `FileFacts.symbols`, the doc of `Import.exported`, the fact format at 3, and `symbols: None` wherever facts are built

## 3. TypeScript

- [x] 3.1 The declarations, with each member of a class as `Class.member`, overloads and merged declarations joined
- [x] 3.2 The uses: references, JSX tags, types, `typeof`, `this.member`, and the writes of top-level variables
- [x] 3.3 What the file uses at load
- [x] 3.4 The local names of imports and of exports
- [x] 3.5 The fixtures and their snapshots, the scenarios of the `typescript` delta, and the cold index of vite measured against 95 ms

## 4. The graph and the walk

- [ ] 4.1 The graph holds the declarations, their uses, what each file uses at load, the local names of imports and exports, with the snapshot layout at 6
- [ ] 4.2 The walk by name: the start, one hop in the file and through importers and barrels, members through their class, the load, contracts of declarations, files without symbols, and depth
- [ ] 4.3 The answer: the header, the callers and the load of each entry, the notes and the JSON, with the scenarios of the `queries` delta

## 5. MCP, the command line and the plugin

- [ ] 5.1 `names` in the MCP `impact` tool, its description and the server instructions, with the scenarios of the `mcp` delta
- [ ] 5.2 `--name` on `gsttt impact`, with the scenarios of the `cli` delta
- [ ] 5.3 The plugin skill and `docs/architecture.md`

## 6. Measure

- [ ] 6.1 The four tests questions of vite asked by name: the callers listed and the tests among them, against the answer per file
- [ ] 6.2 The cold index of vite against the 95 ms of `6a8a111`
