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

## 6. Round two

- [x] 6.1 Python: the bounds of the link are pinned by the reason each leaves (`levels deep`, `ways up`, `include each other`), so a swapped string or an `Unsupported` that is dropped fails a test
- [x] 6.2 Core: `Reading::as_prefix`, used by the Python parse, the TypeScript analyzer and the Rust sites, and `CallRule::behind`, used by the TypeScript and Rust connectors, in place of the matches of the same reading
- [x] 6.3 Core: `serves_http` moves out of the TypeScript connector, and the Rust connector blinds a call behind a prefix that is not text only for a rule that serves HTTP routes, as the TypeScript one does
- [x] 6.4 Python: a mount whose prefix is not text is tested before the cycle and the bounds, and a mount reports the callee it is written as, so the parse and the link report the one site
- [x] 6.5 Python: no match arm lets a default stand for a case, and `written(prefix).unwrap_or_default()` is gone
- [x] 6.6 Python and TypeScript: a spread or an unpacking holds what it may give (a property, a keyword, a key, an argument at or after it)
- [x] 6.7 Python: "A blueprint registered with a prefix and without is served and unread", "A mount whose prefix is not text is not also the cycle it closes", "An unpacking may carry a key or a keyword" and "A Flask route whose options are unpacked serves any method"
- [x] 6.8 TypeScript: "A spread may carry a property", and `READER_VERSION` goes from 7 to 8
