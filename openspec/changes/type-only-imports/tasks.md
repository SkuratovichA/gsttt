# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The analyzer

- [x] 2.1 `crates/lang-ts/src/analyze/symbols.rs`: `Recorder` records whether the file uses a name outside every type
- [x] 2.2 `crates/lang-ts/src/analyze.rs`: the walk keeps the names an import takes from a package an import rule reads and applies the rules to those used as a value once it is done; type names are walked as types; re-exports and import types reach no import rule
- [x] 2.3 `crates/lang-ts/src/project.rs`: `READER_VERSION` 6 to 7

## 3. The rule

- [x] 3.1 `crates/core/src/rules.rs`: the doc comments of `ImportRule` and the pinned schema
- [x] 3.2 `crates/lang-ts/packs/prisma.toml`: the comment above the import rule

## 4. The tests

- [x] 4.1 Fixtures and snapshots of the three scenarios that change, and the table-driven unit tests of the value positions and the other positions

## 5. The review

- [x] 5.1 A use through `this.field` is no use of the import; `import A = Tenant.B` is one; the unit tests are two tables, one per scenario of `Uses of an imported name as a value`
- [x] 5.2 The text of the pack field, the README and the coverage answer follow the new rule
