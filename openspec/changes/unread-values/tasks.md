# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `Reading`, `ValueExpr::read` with `eval` as its text, `Prefix::reading` with `Prefix::read` on top
- [x] 2.2 `crates/core/src/rules.rs`: `Site::holds` in place of `Site::has_arg`, and a literal that stands in only for a value the site does not hold
- [x] 2.3 `crates/core/src/rules.rs`: a decorator whose own `prefix` is unread is a blind spot
- [x] 2.4 `crates/core/src/engine/keys.rs`: `FACT_FORMAT` one up, because cached facts of an unchanged file would keep the old reading

## 3. The connectors

- [x] 3.1 Python: `Site::holds` for a call, an embedded document and an assignment, and a prefix that is not text leaves the route unread
- [x] 3.2 Python: the link follows no way through a router whose own prefix is not text, and the connector goes one up
- [x] 3.3 Rust: a resource whose path is not text is an unread site, and the connector goes one up
- [x] 3.4 TypeScript: `Site::holds` for an argument and a property of an argument

## 4. The scenarios

- [x] 4.1 Core: tests for "A literal stands in only for a value the site does not carry" and "A decorator whose own prefix is not text is unread"
- [x] 4.2 Python: tests for "A router whose prefix is not text serves none of its routes", "A class decorator whose prefix is not text leaves its routes unread" and "A Flask route whose methods are not text serves any method"
- [x] 4.3 Rust: a fixture and a test for "An actix resource whose path is not text serves nothing"
