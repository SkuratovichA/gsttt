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

## 6. Review fixes

- [x] 6.1 A star re-export never drops its importer: it holds the importer with no names when the file has nothing left to pass on, and the importers of the barrel are measured against the names it exports on every star hop
- [x] 6.2 An edge into a file whose exports are unknown is followed at file level, whatever names it carries; Rust records the source name of an aliased `use`
- [x] 6.3 Tests for a held file that becomes whole, `export * as ns`, `default` through a star, unknown exports inside a star chain, an empty X(T), and a proptest oracle over random trees
- [x] 6.4 The `re-exports` reason names the source name and its alias; X(T) is built only for a file with an `export *` line and only when a name of it is wanted, whether it exists is asked without collecting a name, and any other file lends the list of the graph
- [x] 6.5 The TypeScript linker does not forward `default` through `export *`, pinned against the core by a test; one `owned` for the tests; the cut after three names comes from `summary`
- [x] 6.6 A named re-export of a name the barrel does not export is reached like a use of it, as the oracle of 6.3 requires
- [x] 6.7 A cycle of `export *` is pinned through the linker and the walk on one fixture, and a router required in place has a test and a scenario
- [x] 6.8 The Python and the Rust connector are pinned to leave `exports` unknown, so that a reader of `__all__` or `pub` that flips them to by-name has to be decided and tested in the walk
- [x] 6.9 A file the walk reached stays in the answer whatever its reason can name: a list of names the graph cannot spell becomes the whole file, a file or a contract it does not hold is left empty, and only a file without a path is left out
