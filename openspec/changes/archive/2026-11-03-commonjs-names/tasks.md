# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The TypeScript connector

- [x] 2.1 The names a CommonJS file exports, and the forms that leave them unknown
- [x] 2.2 A `require` at the top level binds its locals, and `Import.exported` holds them
- [x] 2.3 `READER_VERSION` 22 to 23

## 3. The walk

- [x] 3.1 One loop for the imports that bind names, used by a require that binds
- [x] 3.2 A require that binds `default` reads as the module, and so does a default import of a file that lists none
- [x] 3.3 A require inside a function takes a name where the file reads it, and the note `HeldModule`

## 4. The scenarios

- [x] 4.1 TypeScript: a fixture and a test for each scenario of "CommonJS exports" and "A require binds its names"
- [x] 4.2 Queries: a test for each new scenario of "Loads that name what they take", and one for the note in the CLI

## 5. The worst listings of the field run

- [x] 5.1 A local that stands for a default is no namespace, and a top-level name counts the files it holds under `HeldModule`
- [x] 5.2 `module.exports = local` lists the keys of a local object, and an empty `module.exports` lists what is added to it
