# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The rule

- [x] 2.1 `crates/core/src/rules.rs`: `CallRule::receiver_types`, and the pack schema snapshot
- [x] 2.2 `crates/lang-python/src/parse.rs` and `crates/lang-rust/src/sites.rs`: a rule with receiver types reads nothing

## 3. The connector

- [x] 3.1 `crates/lang-ts/src/analyze/roots.rs`: watch the receiver types of the call rules, and answer `is_typed(types, name)`
- [x] 3.2 `crates/lang-ts/src/analyze.rs`: a call rule with receiver types reads only a receiver of one of them

## 4. The pack

- [x] 4.1 `crates/lang-ts/packs/kafka.toml`, registered in `connector.rs`, and the blind spots of `queue` in `provides.rs`

## 5. The scenarios

- [x] 5.1 Fixtures `kafka` and `kafka-absent`, and a test for each scenario of the TypeScript requirements
- [x] 5.2 A test per connector for the Python and Rust requirements

## 6. The docs

- [x] 6.1 README: the limit in the TypeScript list
