# Tasks

## 1. Impact

- [x] 1.1 HTTP key matching with `http_prefixes`, with a property test for symmetry
- [x] 1.2 The fixpoint over reverse import edges and contract crossing, with distances, first reasons and a depth limit
- [x] 1.3 The impact answer: ordering, the test group, deleted and unknown inputs, possible misses

## 2. The other questions

- [x] 2.1 `dependents` and `dependencies` with edge kinds, packages, builtins and unresolved imports
- [x] 2.2 `path` through imports and contract hops
- [x] 2.3 `contract` by canonical key and by free text
- [x] 2.4 `coverage` and `status`

## 3. Answers

- [x] 3.1 Text rendering: relative paths, `path:line`, lists cut at 30, empty results with what was searched, the coverage and freshness footer
- [x] 3.2 JSON for every answer, uncut

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("queries", …)]` for every scenario of the `queries` spec
- [x] 4.2 `cargo nextest run -p gsttt-core`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 An HTTP prefix is removed from one path only, and a path that is only a prefix is no path
- [x] 5.2 A lookup compares an HTTP key with the routes that have a segment count it can meet, not with every route of the index
- [x] 5.3 The properties of HTTP matching draw their prefixes from the vocabulary of the keys and draw more cases
- [x] 5.4 `dependencies` of a file that failed to parse says so with the failure, and of one read past syntax errors that imports may be missing, in place of saying that it imports nothing
- [x] 5.5 `impact` keeps the paths the index does not hold in a map, so that thousands of unknown paths cost a lookup each
- [x] 5.6 Possible misses are ordered by path, line and specifier, and so are the unresolved imports of one file, whatever order the connector listed the imports in
- [x] 5.7 The share of resolved imports is computed from the counts as a whole percentage rounded down, so that 29 of 100 reads `29%`, and the JSON footer carries the count behind it
- [x] 5.8 `impact` and `path` cross a contract with one function that takes the direction, and a path hop over a `mod` declaration is named from the edge and the label the dependents use
- [x] 5.9 The architecture document words an empty answer as the spec does
- [x] 5.10 The tie-breaks of `impact` and `path` are written into the spec and pinned by tests: the smallest path first, an import before a contract from one file, contracts in key order and a file's first site by line
- [x] 5.11 A file that serves and calls one contract, and a declarer or a caller reached after a server, are pinned by tests in both walks
- [x] 5.12 `contract` returns an error for a blank query in place of listing every contract
