# Proposal: Read a destructuring of a module as the names it takes

## Why

A destructuring of a module takes the names it writes, yet it counts as a use of every name of the module. `import utils from './utils.cjs'; const { createMap, constants } = utils;` makes the file a caller of every export of `utils.cjs`, so `impact utils.cjs --name escape` lists files that never touch `escape`. An answer that lists every mention is grep, and the extra files cost tokens.

## What Changes

- TypeScript: `const { a, b: c } = lib`, where `lib` is a name and the pattern has only static keys and no rest element, uses `lib.a` and `lib.b` as those member chains do, and not `lib` whole. A rest element or a computed key may take any name, so it stays a use of `lib`. A default value in the pattern is still read.
- A destructuring of a call, an `await`, a member chain such as `lib.sub`, or a local object is read as before.
- Versions: `READER_VERSION` of the TypeScript connector goes from 25 to 26, because the facts of the same text change.

## Impact

- Capabilities `typescript` and `queries`.
- `visit_variable_declarator` in `crates/lang-ts/src/analyze.rs`.
- A tree that spells every use as `lib.a` gives the same answers.
