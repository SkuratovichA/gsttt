# Proposal: Name a table only for an imported name the file uses as a value

## Why

The Prisma pack names the table of every name a file imports from `@prisma/client`, because the package exports one type per model. Measured on two real repositories, most of the files counted as users of a table imported the type of a model and never queried the table. `tsc` already reports what a change of a model breaks in a type, and an agent that asks who uses a table needs the files that query it, which the chain rules of the pack above the import rule find.

## What Changes

- A name imported from a package that an `[[imports]]` rule reads names its contract only when the file uses the local name as a value. A use is a reference outside every type: a type annotation, a type argument, a type alias, an interface with its `extends`, an `implements` clause, `typeof X` written inside a type, the type of `satisfies` or `as`, and an ambient signature are types. A call, an argument, `Name.member`, `typeof X` as an expression, a class `extends`, an enum initializer and `export default Name` are values.
- `import type { X }`, `import { type X }` and an import type `import('@prisma/client').Tenant` name nothing. A value import that the file uses only in types names nothing.
- A re-export is not a use: `export { X } from 'pkg'`, `export type { X } from 'pkg'` and `export { X }` of an imported binding name nothing.
- The fact stays at the line of the import and keeps the name as the package exports it, not as the file renames it.
- The requirement `Imported model types` moves to `Imported model values` and `Import types reach the import rules` is removed, because their scenarios change their names and `openspec archive` refuses a modified requirement that drops a scenario. `A namespace import of the client names nothing` is carried word for word.
- The version of the reader goes from 6 to 7, because the facts it produces for the same text and packs change. `FACT_FORMAT` stays: core applies the rule as before, to the names the connector hands it.

## Impact

- Capability `typescript`.
- `crates/lang-ts/` (the analyzer applies the import rules once the walk is done, `Recorder` records the use of a name as a value, the Prisma pack's comment, fixtures, tests and snapshots) and the doc comments of `ImportRule` in `crates/core/src/rules.rs`, with the pinned pack schema.
- Cost: impact on `schema.prisma` no longer reaches a file that only imports the type of a model. The types of the models are types only, so after this change a model is named by the queries on it. An enum that a file uses as a value still reads as a table that no model declares, as before; enums wait for a later change, because lang-ts cannot see `schema.prisma`.
- Indexes built by an earlier version are parsed again once.
