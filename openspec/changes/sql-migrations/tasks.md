# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The glob helper

- [x] 2.1 `crates/core/src/glob.rs`: `glob_set` and `GlobError`, with unit tests
- [x] 2.2 `crates/contracts/src/openapi.rs`, `crates/config/src/external.rs`: use it, keeping their messages

## 3. The connector

- [x] 3.1 `crates/contracts/src/sql.rs`: the `migrations` option, a migration declares every table it names once, `version` 1 to 2

## 4. The scenarios

- [x] 4.1 A test and a fixture for "A migration declares every table it names"
- [x] 4.2 A test for "A query outside the migrations calls its tables"
- [x] 4.3 A test for "The migrations option replaces the default globs"
- [x] 4.4 A test for "An empty migrations list reads no migration"
- [x] 4.5 A test for "An invalid migrations glob is an options error"
- [x] 4.6 A test for "The singular migration directory is a migration"
- [x] 4.7 A test and a fixture for "SQL embedded in a file under migrations calls its tables"
