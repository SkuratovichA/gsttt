# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `EmbedRule::computed` with `ComputedText`, and a unit test

## 3. The pack

- [x] 3.1 `crates/lang-ts/packs/sql-drivers.toml`, registered in `crates/lang-ts/src/connector.rs`
- [x] 3.2 `crates/lang-ts/src/provides.rs`: the blind spots of the SQL drivers

## 4. The scenarios

- [x] 4.1 Fixtures `sql-drivers` and `sql-drivers-absent`
- [x] 4.2 Connector tests in `crates/lang-ts/tests/typescript/raw_sql.rs` and tests through the binary in `crates/gsttt/tests/cli/raw_sql.rs`
