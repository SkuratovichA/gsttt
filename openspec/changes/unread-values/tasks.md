# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The core

- [x] 2.1 `crates/core/src/rules.rs`: `Reading`, `ValueExpr::read` with `eval` as its text, `Prefix::reading` in place of `Prefix::read`
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

## 5. The fix round

- [x] 5.1 Python: a Flask `url_prefix` of a registration stands in for a blueprint prefix that is not text, and an unread own prefix leaves the unread site at the router that holds it: at each route for the router a way starts from, at the include below it for a router further up
- [x] 5.2 Python: the cuts of the link reach coverage as unread sites, a prefix that is not text as non-literal and a cycle or a bound as unsupported, and a mount the parse reported is not reported again
- [x] 5.3 Core and Python: "written, absent or unread" is one `Reading`, the keyword of a mount is read through the `ValueExpr` of the site, and `prefix=None` is no prefix
- [x] 5.4 TypeScript: the prefix of a receiver is read with `Prefix::reading`, and a rule that serves routes finds a blind spot where it is not text, for a receiver made in the same file and in another
- [x] 5.5 Core: `UnresolvedSite::non_literal` in place of the inline constructions of a site whose value is not a literal, and the doc of `method` says a method that is held and not text is any method
- [x] 5.6 Tests: "A registration prefix stands in for a blueprint prefix that is not text", "A mount whose prefix is not text is one unread site", "Routers that include each other leave the include an unread site" and the three scenarios of "Receivers whose prefix is not text"
