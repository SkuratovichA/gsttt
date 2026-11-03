# Proposal: Read the types a JSDoc comment gives in a JavaScript file

## Why

A JavaScript project without TypeScript documents its types in JSDoc, and the compiler reads them there. In `a.compare(b)` with `@param {ESQueryParsedSelector} a` the call is a use of `ESQueryParsedSelector.compare`, but gsttt reads `a` as a value of an unknown class, so the question about that member misses a real caller. A type that a comment imports with `@import` or `import('x')` is no import of the file either, so the file is not listed as a dependent of the module that declares it.

## What Changes

- TypeScript, in `.js`, `.cjs`, `.mjs` and `.jsx` files only: an `@import` tag and every `import('x')` or `import ('x').Name` in a type of a JSDoc tag is a type-only import, as `import type` is. `import('x')` is the value `x` exports, so it takes `default`; `import('x').Y` takes `Y` and binds `Y`, unless the code of the file binds `Y`, because a comment never shadows code. `@typedef {import('x').Y} Z` binds `Z` to `Y` of `x`, and `@typedef {import('x')} Z` binds `Z` to `default`. An import with no local is at the line of the token the comment sits above, so the declaration under it counts as using it.
- A type that a `@param`, `@returns`, `@return`, `@type` or `@this` tag names, a plain class or `import('x').Y`, is a use of that class by the declaration under the comment.
- A parameter documented with `@param {T}`, where `T` is one plain class, holds that class, so a member read from it is a use of the member of the class. The comment of a statement, a method or an object property documents the parameters of the first function in it. A name inside a destructured parameter is typed by the dotted `@param` that names it, `config.linter`.
- A `/** @type {T} */` above a `const` or `let` of one name gives it that class.
- A type that is no plain class gives no class: a union of two classes, a generic, `Object`, `Function`, `Array`, `Promise`, a primitive wrapper and a lower-case name. `@example`, `@description` and every tag that is not a type tag are never read, and a TypeScript file gets no new fact.
- Queries: a type reads no member of a class it names. Asked about a member, a file that names the class only in a type is a holder of the class, and its declarations are callers only where the file reads the member.
- Versions: `READER_VERSION` of the TypeScript connector goes from 32 to 33, because the facts of the same text change. `FACT_FORMAT` does not move: no field of a fact is new, and the core reads the facts as before, except that it reads no member in a type.

## Impact

- Capability `typescript`, new requirement "JSDoc types in JavaScript". Capability `queries`, new requirement "A type reads no member", in `crates/core/src/query/walk/names.rs`.
- `crates/lang-ts/src/analyze/jsdoc.rs` is new. `Constants::collect` and `Scopes` in `crates/lang-ts/src/analyze/constants/` take the documented classes, and `Analyzer` and `Recorder` take the imports.
