# Tasks

## 1. Facts

- [x] 1.1 `FileFacts.exports` and `Import.exported`, left out of the JSON when empty; `FACT_FORMAT` 2
- [x] 1.2 The TypeScript connector records a file's own exports, the aliases of its named re-exports, and unknown exports for CommonJS, `export =` and files read past errors; connector version 2; snapshots reviewed

## 2. Graph

- [x] 2.1 Interned names, a binding per edge that merges with it, and each file's own exports; snapshot layout 5
- [x] 2.2 The build fills them from the resolved imports, and the testing `Tree` can give an import its names and a file its exports

## 3. Neighbours

- [x] 3.1 `dependents` and `dependencies` carry the names taken, in text and in JSON, with the cut after three names

## 4. Impact

- [x] 4.1 The walk follows names: names per file, the next frontier for growth, X(T) closed over cycles, unknown exports falling back to the whole file, names the target does not export, contracts crossed only from whole files
- [x] 4.2 `Reason::Imports` with names and `Reason::ReExports`, in text and in JSON
- [x] 4.3 `docs/architecture.md` describes the walk by names

## 5. Proof

- [x] 5.1 A test for each scenario of this change
