# Proposal: A class bound by require gives its class

## Why

In CommonJS the class is usually bound as `const ContentType = require('./content-type')`. Then `const contentType = ContentType.from(header)` and `contentType.toString()` read as no use of `ContentType.toString`, so the question about that member misses a real caller. A static call on a class already gives the class when the head is a class of the file, an import by name, a default import or a destructured require; only the plain require is left out.

## What Changes

- TypeScript: an upper-case name bound by `const Name = require('./x')` is a class head, so a static call on it gives that class, in a function and at the top level. A lower-case name and a namespace import stay out.
- Versions: `READER_VERSION` of the TypeScript connector goes from 31 to 32, because the facts of the same text change.

## Impact

- Capability `typescript`, requirement "Members read through what a factory gives".
- `declared_heads` and `class_heads` in `crates/lang-ts/src/analyze/constants.rs`.
