# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The rule

- [x] 2.1 `crates/core/src/rules.rs`: `CallRule::test_double`, and the pack schema snapshot
- [x] 2.2 `crates/lang-ts/packs/nestjs-config.toml`: `test_double = "ConfigService"`

## 3. The connector

- [x] 3.1 `crates/lang-ts/src/analyze/doubles.rs`: the pre-walk that finds the stand-ins of a test file
- [x] 3.2 `analyze.rs`, `source.rs`, `project.rs`: the tags reach the analyzer, and a rule whose token is doubled reads nothing

## 4. The scenarios

- [x] 4.1 Fixture `nestjs-config-double` and a test for each scenario of "A service a test provides itself"
- [x] 4.2 Unit tests of `doubles.rs` for each form of stand-in and each form that is none

## 5. The docs

- [x] 5.1 README: the limit in the TypeScript list
