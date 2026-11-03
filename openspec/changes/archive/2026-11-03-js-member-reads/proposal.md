# Proposal: More forms of require bind, and a builtin has no member of the repository

## Why

Asked about one method, gsttt lists a file that cannot call it. On four CommonJS repositories 80 files were listed beyond the real callers, and 19 of them fall into classes that can be told apart without types: a `require` that the connector does not bind, so the file counts as taking the whole module; a test whose callback holds a `require` of a class and never reads the asked member; and a member read on a value that is a builtin, such as `Object.entries(x)`, `path.normalize(p)` or `new WeakMap().get(k)`, which says nothing about a class of the repository.

## What Changes

- TypeScript: a `require` in either branch of a conditional, a `require` assigned to a local of the file at any depth, and a `require` read by a computed member bind their names, as a `require` at the top level does.
- TypeScript: a member read on a global builtin the file never binds, on a local bound to a Node module, and on a local every value of which is a global builtin or made by a Node module, is no read. A name the file binds in any scope, or a local also given another value, is unknown, and its reads stay.
- Queries: a class taken by a `require` below the top level is used where the file uses the member or reads it on some value, and not by a use of the class alone.
- Versions: `READER_VERSION` of the TypeScript connector goes from 34 to 35, because the facts of the same text change.
- Not changed: undici `lib/core/request.js` is listed for `FormData.get` because of a bare `require('../web/fetch/formdata')` inside a function, which loads a module for its effect and is none of the forms above.

## Impact

- Capabilities `typescript`, requirement "A require binds its names" and two new requirements, and `queries`, one new requirement.
- `load_forms` and `Taking::Assigned` in `crates/lang-ts/src/analyze.rs`, `crates/lang-ts/src/analyze/receivers.rs`, `is_global_builtin` in `crates/lang-ts/src/builtins.rs`, and `by_name` in `crates/core/src/query/walk/names.rs`.
