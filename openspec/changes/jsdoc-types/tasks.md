# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The TypeScript connector

- [x] 2.1 `analyze/jsdoc.rs` reads the JSDoc comments of a JavaScript file: the imports, the `@param` and `@type` classes
- [x] 2.2 The imports of the comments are type-only imports of the file and bind their names
- [x] 2.3 `Scopes` gives a documented parameter or declaration its class
- [x] 2.4 `READER_VERSION` 32 to 33
- [x] 2.5 An import type with no name takes `default`, a qualified one binds its name unless the code does, and the types of the tags are uses by the declaration under the comment

## 3. The queries

- [x] 3.1 `use_taken`: a type that names a class reads no member of it

## 4. The scenarios

- [x] 4.1 TypeScript: a fixture, a test per scenario, and a property test of the type reader
- [x] 4.2 CLI: `impact` with `--name` lists the file that calls a member of a documented parameter
