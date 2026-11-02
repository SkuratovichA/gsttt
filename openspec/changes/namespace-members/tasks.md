# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The walk

- [x] 2.1 `crates/core/src/query/walk/names.rs`: a namespace import binds the name it takes as the member of its local, and a local refers to a use of itself and of `local.member`
- [x] 2.2 `docs/architecture.md`: the sentence on a namespace import

## 3. The scenarios

- [x] 3.1 Core and CLI: tests for "A namespace import is a caller only of the names it uses" and "A namespace used as a value uses every name"
- [x] 3.2 Core: tests for "A load reads through a namespace by the names it uses" and "A name goes on through a namespace a barrel exports"

## 4. The connector

- [x] 4.1 `crates/lang-ts`: `visit_ts_type_name` and `visit_jsx_member_expression` record the whole name through `Recorder::chain`, and `READER_VERSION` becomes 19
- [x] 4.2 `lang-ts` tests for "A qualified type name is used by its whole name" and "A JSX member is used by its whole name"; the CLI test covers a type, a type query, a tag and a type-only namespace
