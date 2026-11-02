# Proposal: Prisma clients named by a transaction callback

## Why

`prisma.$transaction(async (db) => { await db.user.findMany() })` names a client by its callback parameter, and the author picks the name. The `prisma` pack lists `tx` and `db` among its roots, which catches most of them by luck and nothing else: a callback whose parameter is `client` or `trx` leaves its tables out of the graph, with no word about it.

## What Changes

- Core: a chain rule may list `callback_roots`, patterns of calls (matched like a callee, with `*` for one segment) whose callback receives a root. The first parameter of a function given as the first argument of such a call starts a chain of the rule. The field is additive, left out of a serialized rule when empty, and read by the TypeScript connector only; Python and Rust read the plain `root` list of a chain rule and have no such binding.
- Pack: the `prisma` pack lists `*.$transaction`, `*.*.$transaction` and `*.*.*.$transaction`.
- TypeScript: the walk that finds the names declared with a listed type also declares the first parameter of such a callback, with the pattern of the call as its type. A pattern has a `*` or a `$` and so never equals a type name. The parameter is a plain identifier, with or without a type annotation; a destructured parameter names no client. The per-file policy of the other roots stays: a name the file binds anywhere else to anything but `this` or a root is no root.
- Cost: a call is checked only when some pack lists callback roots, by the last segment of its callee first, so a call no pattern ends with allocates nothing.
- Text: the limits listed for the clients a file reaches (`provides.rs`, `README.md`) name the callback parameter of `$transaction`.

## Impact

- Capability `typescript`. No other capability documents the fields of a chain rule.
- `crates/core/src/rules.rs`, the pack schema snapshot, `crates/lang-ts/packs/prisma.toml`, `crates/lang-ts/src/analyze/roots.rs`, `crates/lang-ts/src/analyze.rs`.
- No version bump: the pack content is part of the module state, so a changed built-in pack re-parses every file of the TypeScript projects that use it, and a custom pack that does not list `callback_roots` reads as before. Not checked against a stale cache beyond this reading of the key.
- A tree that calls `$transaction` with a callback whose parameter is not `tx` or `db` gains the tables the callback reads.
