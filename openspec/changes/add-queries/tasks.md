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
