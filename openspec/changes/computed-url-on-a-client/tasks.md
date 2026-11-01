# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: an HTTP call checks the joined target for placeholders alone, with or without a prefix, and skips a non-URL only with no prefix
- [x] 2.2 `crates/core/src/rules.rs`: unit tests for an empty base, a base of placeholders, a literal base and no base

## 3. The scenario

- [x] 3.1 TypeScript: a test and a fixture for "A client with no base URL leaves a computed URL unread"
