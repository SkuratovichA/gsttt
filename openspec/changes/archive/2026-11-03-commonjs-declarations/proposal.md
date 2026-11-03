# Proposal: Declare the names a CommonJS file exports in place

## Why

A CommonJS file that writes its exports in place declares nothing. `exports.run = function () {}` and `module.exports = { run() {} }` list `run` as an export since the change `commonjs-names`, but `impact lib.js --name run` answers that the file declares no `run`, so the names that older Node code exports cannot be asked about. eslint holds 726 such exports, undici 184 and mocha 79.

## What Changes

- TypeScript: a top-level `exports.x = value`, `module.exports.x = value` and `exports.a = exports.b = value` declare the names they give, over the lines of the statement. A function or an arrow is declared like a function and what its body uses counts for the name and not as the file loads. A class is declared under the exported name with its members, whatever name the class expression carries. Any other value is declared like a `const`.
- TypeScript: `module.exports = function run() {}` and `module.exports = class Foo { go() {} }` declare the name the expression carries, with the members of the class, and export `default` from that name, as `module.exports = Foo` does for a class. An anonymous one declares nothing.
- TypeScript: `module.exports = { run() {}, stop: function () {}, limit: 5, Bar: class {} }` declares each key whose value is written in place, each with the lines and the uses of its own property. A key that holds a local or a `require`, a computed key and a spread declare nothing, and what they hold is still read as the file loads.
- TypeScript: `exports.run()` and `module.exports.run()` are a use of `run` in the file that declares it, when the file binds neither `exports` nor `module`.
- Only top-level statements declare: `if (x) exports.y = 1` stays as it was.
- Versions: `READER_VERSION` of the TypeScript connector goes from 24 to 25, because the declarations of the same text change. No fact changes shape.

## Impact

- Capabilities `typescript` and `queries`.
- `Recorder` in `crates/lang-ts/src/analyze/symbols.rs`, `commonjs_export` and `visit_object_property` in `crates/lang-ts/src/analyze.rs`, `named_callable` in `crates/lang-ts/src/shape.rs`.
- A tree with no CommonJS gives the same answers as before.
