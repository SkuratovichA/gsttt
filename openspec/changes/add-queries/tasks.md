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
