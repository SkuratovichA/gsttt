# Proposal: Pass names on through a CommonJS barrel

## Why

Asked about a name, gsttt lists the files that call it. Through a barrel written in CommonJS it loses them: `module.exports = { one: require('./lib/one') }` leaves the barrel as a file that loads `./lib/one` and takes nothing from it, so the answer stops at the barrel and misses the file that calls `one`. The same barrel written `export { default as one } from './lib/one'` answers right, so the walk of the core does the right thing and only the facts of the connector differ.

## What Changes

- TypeScript: a CommonJS export whose value is `require('x')`, `require('x').y` or `local.y` under a top-level `const local = require('x')` is read as `export { default as name } from 'x'` or `export { y as name } from 'x'`, and the `require` that it reads is no load of the module.
- `module.exports = require('x')`, an object with `...require('x')`, and a call that hands `exports` or `module.exports` together with `require('x')`, as `Object.assign(module.exports, require('x'))` and `__exportStar(require('x'), exports)` do, are read as `export * from 'x'`, and the first also as a re-export of `default`. The exports of the file stay unknown as before.
- A spread gives no use to a neighbouring key.
- Asked about `helper` of a module that a barrel reaches by `lib.many.helper()`, the importer still lists every function that reads a member of `many`, as the same barrel written with `export` does: a file-level answer stays right.
- Versions: `READER_VERSION` of the TypeScript connector goes from 33 to 34, because the facts of the same text change. `FACT_FORMAT` stays.

## Impact

- Capability `typescript`.
- `export_value`, `commonjs_export`, `exports_handed_to` and the export-from visitors in `crates/lang-ts/src/analyze.rs`, and `Constants::required_spec` in `crates/lang-ts/src/analyze/constants.rs`.
- A tree that names each module as before gives the same answers.
