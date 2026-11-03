# Proposal: Record the members that hold a value, and the reads through them, in TypeScript

## Why

The walk by name follows a member that holds a value of a class to the files that read through it, but only Python records the facts it needs. A file that reads `ctx.logger.error()` calls `Logger.error` through an interface property that names its class, and TypeScript records neither the property nor the read of `logger.error`, so the file is not listed. The same shape is common in NestJS code, where a context object holds a logger or a client, as in `NotificationContext.amplicationLogger`.

## What Changes

- TypeScript: a field annotated or constructed with a class, a parameter property, an interface property, a property of a type alias of an object literal, a getter and a method with a return annotation, and an interface method with a return annotation are recorded in `holds` when their written type has one head that is a class. `A | null` and `A | undefined` hold `A`, `Promise<T>` of a method holds `T`, and `A | B`, a primitive, a builtin, `this` and the class itself hold nothing. A private member is recorded under its `member_name` form.
- TypeScript: a member read right on another is recorded as a read of the pair `on.member`, so `ctx.logger.error()` reads `ctx.logger` beside `logger.error` when `ctx` is a member of `this`, and reads `logger.error` when `ctx` is a parameter. `!` and `?.` are seen through, and `x.get().name` and `(await x.get()).name` read `get.name`. A chain on a value the file shows is a builtin records no pair, and the name at the root of a chain is never the `on` of a pair.
- Versions: `READER_VERSION` of the TypeScript connector goes from 36 to 37, because it reads and stores more. `FACT_FORMAT` and `LAYOUT` stay, because the shape of `Symbols` does not change.

## Impact

- Capability `typescript`.
- `Recorder` and `finish` in `crates/lang-ts/src/analyze/symbols.rs`, `visit_member_expression` in `crates/lang-ts/src/analyze.rs`, `READER_VERSION` in `crates/lang-ts/src/project.rs`.
- TypeScript snapshots of files with such members or chains gain a `holds` list or a read of a pair. A tree whose files hold no such member gives the same answers.

## Limits

- A chain broken by a local, `const l = ctx.logger; l.error()`, is not followed.
- No type inference is added: only a type the file writes makes a member hold a value.
- A file is judged by what it reads as a whole: a pair read on another value counts for it, as a single read does today.
