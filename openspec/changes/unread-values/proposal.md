# Proposal: Tell a value a site lacks from one it cannot read

## Why

A rule's value expression gave `None` both for a site that carries no such value and for a site that carries a value no connector could read as text, so the two were treated alike. The literal that stands in for a missing value then stood in for a value that was there too, and a prefix that was there but not text was read as no prefix. `APIRouter(prefix=settings.API_PREFIX)` served its routes at the path they were written with, `f('/x', options)` read the method `GET` of a rule `$arg2.method|GET`, a Flask `@app.route("/x", methods=METHODS)` was a `GET`, and `web::resource(PATH)` served `/`. Each of them is a contract at a path or method that nothing serves, which a graph that answers what a change affects must not draw.

## What Changes

- Core: a value expression is read to `Reading::Text`, `Reading::Absent` or `Reading::Unread`. `ValueExpr::eval` stays as the text of that reading, and `Prefix::reading` is the reading of a prefix, which `Prefix::read` keeps as text for the callers that read a client's base.
- Core: `Site::has_arg` becomes `Site::holds`, which says for any source of a value whether the site holds one it cannot read as text. A literal alternative stands in only for a value the site does not hold.
- Core: a decorator whose own `prefix` is unread is a blind spot, since joining nothing serves the contract at a path that is not its own.
- Python: a route on a router, a blueprint or a class whose prefix is not text is an unread site and serves nothing. The link follows no way through such a router, so a router included into it is served by no chain that passes it. A Flask `methods` that is not text is read as any method.
- Rust: a `web::resource` whose path is not text is an unread site and serves nothing. A client whose base is not text is read as before.
- TypeScript: implements `Site::holds` for an argument and a property of an argument. The prefix of a `@Controller` and the base of a client are read as before.
- The requirements "A literal alternative", "A decorator prefix that is not text" (`contract-formats`), "Prefixes that are not text" and "Flask routes" (`python`) and "Actix resources that are not text" (`rust`) get their scenarios.
- Versions: `FACT_FORMAT` goes from 11 to 12, because the rule code is not in the key of cached facts and an unchanged file would keep the facts read the old way. The Python connector goes from 1 to 2 and the Rust connector from 2 to 3, because what they hand the engine changes: a Python router keeps a prefix that is not text, a Rust resource that is not text yields no fact. `READER_VERSION` of the TypeScript connector stays: it stores the same types as before, and the new `FACT_FORMAT` makes every file be read again.

## Impact

- Capabilities `contract-formats`, `python` and `rust`.
- `crates/core/src/rules.rs`, `FACT_FORMAT` in `crates/core/src/engine/keys.rs`, the Python and Rust connectors (`site.rs`, `parse.rs`, `link.rs`, `sites.rs`), and `Site::holds` in the TypeScript connector.
- A tree with no prefix, method or path that is not text gives the same index as before. A tree with one gives fewer facts, each of them an unread site.
