# Proposal: A key of an object literal the file wrote is no read

## Why

Asked about a method, gsttt lists every file that reads a member of that name on a value whose type it cannot tell. A test that writes `const prismaMock = { resource: { findMany: fn } }` and reads `prismaMock.resource.findMany.mockReturnValueOnce()` is listed for every service with a `findMany`, although `resource` and `findMany` are keys the file wrote itself. A query that returns hundreds of files for one method is no better than a grep.

## What Changes

- TypeScript: a name bound exactly once in the file, by `const`, to an object literal gets the shape of that literal, its named keys, nested where a key holds an object literal. The links of a member chain on the name that are keys of the shape are no reads. What follows them is read as before, because a key can hold anything: `deps.svc.place()` with `const deps = { svc: new Service() }` still reads `place`.
- A name bound a second time (a parameter, an inner `const`, an assignment, an import), a cast or `satisfies` on the literal, and a call such as `Object.assign({}, x)` give no shape. A computed key adds no key, and a spread hides what the keys before it hold.
- Limit: destructuring a literal (`const { resource } = prismaMock; resource.findMany()`) stays a read.
- Versions: `READER_VERSION` of the TypeScript connector goes from 35 to 36, because the facts of the same text change. `FACT_FORMAT` stays.

## Impact

- Capability `typescript`, new requirement "A key of an object literal the file wrote is no read".
- `Receivers` in `crates/lang-ts/src/analyze/receivers.rs` and the chain rule in `crates/lang-ts/src/analyze.rs`.
