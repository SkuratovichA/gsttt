# Spec Delta

## REMOVED Requirements

### Requirement: Removed declarations walk the file whole
**Reason**: The old name is known from the importers that were not changed, so a removed or renamed declaration is walked by that name and the file need not be walked whole.
**Migration**: See "Removed declarations are walked by their old name", "Renamed declarations are walked by their old name", "A removed export is walked by its name" and "A move with a removal walks the file whole".

## ADDED Requirements

### Requirement: Removed declarations are walked by their old name
A declaration removed from a file SHALL be walked by its old name, and the importers that still name it SHALL be affected and no other importer of the file. The part of the answer for the file SHALL name it as removed.

#### Scenario: --changed walks the importers of a removed declaration
- **WHEN** `formatId` is removed from `src/format.ts`, and `src/admin/audit.ts` still imports it
- **THEN** `src/admin/audit.ts` is affected, the importer of `formatUser` is not, and `parts` names `formatId` under `removed`

#### Scenario: --changed lists no importer of a removed declaration nobody imports
- **WHEN** `formatId` is removed from `src/format.ts` and no file imports it
- **THEN** the file is listed as changed and no file is affected

### Requirement: Renamed declarations are walked by their old name
A declaration renamed in a file SHALL be walked by its old name and by its new name. An importer changed to the new name SHALL be affected as the importer of a changed declaration, and an importer of another name SHALL NOT.

#### Scenario: --changed walks a renamed declaration by its old name
- **WHEN** `formatId` is renamed to `formatKey` in `src/format.ts`, a file still imports `formatId`, another imports `formatKey`, and a third imports only `formatUser`
- **THEN** the first two are affected and the third is not

#### Scenario: --changed lists only the changed importers of a healthy rename
- **WHEN** `formatId` is renamed to `formatKey` and `src/admin/audit.ts` is changed to import it
- **THEN** `src/admin/audit.ts` is among the changed files and the importer of `formatUser` is not affected

### Requirement: A removed export is walked by its name
A name a file stops exporting SHALL be walked by that name, whether its declaration was removed or kept without `export`, and whether the name is `default`. A removed member of a class that stays SHALL be walked as a member, and a removed name SHALL be passed on by a barrel that re-exports it.

#### Scenario: --changed walks the importers of a declaration that stops being exported
- **WHEN** `export function formatId` becomes `function formatId` in `src/format.ts`
- **THEN** the files that import `formatId` are affected and the importer of `formatUser` is not

#### Scenario: --changed walks the importers of a removed default export
- **WHEN** `export default function` is removed from a file and a file imports its default
- **THEN** that file is affected

#### Scenario: --changed walks the callers of a removed member
- **WHEN** a method is removed from a class that stays, and one file calls it and another calls only a different method
- **THEN** the first file is affected and the second is not

#### Scenario: --changed walks a removed declaration through a barrel
- **WHEN** `formatId` is removed from a file that a barrel re-exports with `export *` and with a named re-export
- **THEN** the importers of the barrel that take `formatId` are affected and those that take another name are not

### Requirement: A move with a removal walks the file whole
A file in which a declaration moves or a top-level statement changes SHALL be walked whole, whether a declaration is removed from it or not.

#### Scenario: --changed walks a file whole when a declaration moves and another is removed
- **WHEN** `formatId` is removed from `src/format.ts` and `formatUser` moves below the helper
- **THEN** every importer of the file is affected
