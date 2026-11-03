# Proposal: List CommonJS exports and bind what a require takes

## Why

Measured against the TypeScript language service on fastify, undici, eslint and mocha, the files that impact lists for a top-level name have recall 94.4% and precision 18.8%, 9.85 extra files for each name, and for a method precision is 39.9%. One cause: a file that assigns `module.exports` has its exports unknown, so the walk takes every name of it for any name asked about, and a `require` takes the whole module, so every file that requires a file is a caller of every name in it.

## What Changes

- TypeScript: the connector lists the names a CommonJS file exports where its form gives them: `exports.a = …`, `module.exports.a = …`, `module.exports = { a, b: c, d() {} }`, and `module.exports = Foo` for a function, a class or a variable that holds one of them, which exports `default`. They stay unknown for an empty object or one with a spread or a computed key, for any other value, for `module.exports = require(…)`, for `exports[key] =`, for `exports` or `module.exports` handed to a call other than the `__esModule` marker, for `export =`, and for a file read past errors.
- TypeScript: a `require` at the top level of a file that binds its result to a plain identifier or to a pattern of plain names binds locals as an import does: `const lib = require('./a')` takes `default` as `lib`, and `const { b, c: d } = require('./a')` takes `b` and `c` as `b` and `d`. Neither declares anything, and a use of `lib.x` is a use of `x` of `lib`.
- Queries: a require that binds `default` reads as the module, so the file uses a name where it uses `local.name`, and a default import of a file that lists no default does the same. A require inside a function takes a name only where the file reads it by that name; a file that does not is counted in a note and not listed.
- Versions: `READER_VERSION` of the TypeScript connector goes from 22 to 23, because the exports and the imports of the same text change.

## Impact

- Capabilities `typescript` and `queries`.
- `Analyzer` and `Recorder` in `crates/lang-ts/src/analyze.rs` and `crates/lang-ts/src/analyze/symbols.rs`, `by_name` in `crates/core/src/query/walk/names.rs` and the notes of `crates/core/src/query/impact.rs`.
- A tree with no CommonJS gives the same answers as before.
