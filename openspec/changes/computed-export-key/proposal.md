# Proposal: A computed export key at the top level leaves the other names known

## Why

`exports[key] = value` and `module.exports[key] = value` made every export of the file unknown, so a walk asked about a name the file only imports took it as passed on, and every importer of the file was listed with no hop counted. In a fastify checkout, `module.exports[Symbol.for('internals')] = { handler }` in one file listed `lib/route.js`, `fastify.js` and three test files for a question about `ContentType`, which none of them uses.

## What Changes

- TypeScript: an assignment to a computed key of `exports` or `module.exports` written as a whole statement of the top level runs once and makes one name, so it leaves the exports known. A literal key exports that name. Any other key exports what the value hands out under its own names: the keys of an object literal, or the variable of an identifier. It never exports `default`.
- A computed assignment inside a function, a loop, a condition or a callback, `Object.assign(module.exports, x)`, `factory(exports)` and `module.exports = { [k]: 1 }` leave the exports unknown as before.
- Versions: `READER_VERSION` of the TypeScript connector goes from 30 to 31, because the facts of the same text change.

## Impact

- Capabilities `typescript` and `queries`.
- `visit_assignment_expression`, `visit_program` and the new `computed_export` in `crates/lang-ts/src/analyze.rs`.
- A tree without a computed key of the exports at the top level gives the same answers.
