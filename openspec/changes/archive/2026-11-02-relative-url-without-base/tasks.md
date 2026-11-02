# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `RelativeWithoutBase`, `Pack::relative_without_base` and `UnreadBase`
- [x] 2.2 `crates/core/src/rules.rs`: `CallRule::behind` tells the three bases apart, and `CallRule::apply` takes the setting of the pack
- [x] 2.3 `crates/core/src/rules.rs`: the target of a request with no base is decided by one function
- [x] 2.4 `crates/core/src/contract.rs`: `has_parent_segment`, shared with `url_join`
- [x] 2.5 `crates/core/src/engine/keys.rs`: `FACT_FORMAT` from 13 to 14

## 3. The connectors and the packs

- [x] 3.1 TypeScript, Python and Rust pass what `behind` gave to `apply`
- [x] 3.2 `relative_without_base` in the `axios`, `fetch`, `requests`, `httpx` and `reqwest` packs
- [x] 3.3 The pinned pack schema and the pack format in `docs/architecture.md`

## 4. The scenarios

- [x] 4.1 Core: tests for the four scenarios of "Relative requests"
- [x] 4.2 Config: tests for "A pack sets what a relative request means" and "A relative setting that is neither value is refused"
- [x] 4.3 TypeScript, Python and Rust: a fixture and tests for each scenario

## 5. The edges of a relative target

- [x] 5.1 `relative_target` drops `./` first, reuses `is_scheme`, and reads ``./${x}/y`` as unread
- [x] 5.2 A base that is there and unread is decided as `rooted`
- [x] 5.3 The unread site of a relative request says why
- [x] 5.4 Rust: a type brought in by a `use` of another crate is no client
- [x] 5.5 The probe table, the segment property test, the Python serves-rule test and the config message
