# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The rule

- [x] 2.1 `crates/core/src/contract.rs`: `PARAMETER`, used by the normaliser, `has_literal_segment` in `crates/core/src/rules.rs` and the matcher
- [x] 2.2 `crates/core/src/query/http.rs`: `segments_meet` needs a shared literal when both paths hold a literal, and the doc comment of `HttpMatcher` says so

## 3. The tests

- [x] 3.1 `crates/core/tests/queries/http.rs`: a test for each new scenario and a property test that a key meets itself with literals made parameters
- [x] 3.2 `crates/core/tests/queries/impact.rs`: a call that shares no literal with a route does not reach it

## 4. The prose

- [x] 4.1 `docs/architecture.md` states the rule
