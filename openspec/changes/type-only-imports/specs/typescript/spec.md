## REMOVED Requirements

### Requirement: Imported model types
**Reason**: A name imported or re-exported from the client names its table only when the file uses it as a value, so the scenarios that named a type import and a re-export change their meaning. The requirement moves to `Imported model values`.
**Migration**: A file that queries a table is still found through the chain rules of the pack. A file that only imports the type of a model no longer calls its table.

### Requirement: Import types reach the import rules
**Reason**: An import type is a type, which no longer names a table.
**Migration**: None. The file that queries the table still calls it through the chain rules.

## ADDED Requirements

### Requirement: Imported model values
A pack MAY name a contract for each name a file takes from a package. With the `prisma` pack, a name the file imports from `@prisma/client` and uses as a value SHALL call the table of that name at the line of the import, as the package exports it and not as the file renames it, except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`. A type, a re-export, a namespace import, a default import, a side-effect import and `export *` SHALL name nothing.

#### Scenario: A model imported as a value calls its table
- **WHEN** `src/models.ts` has `import { BookingStatus } from '@prisma/client'` on line 1 and reads `BookingStatus.CONFIRMED`
- **AND** `src/renamed.ts` has `import { BookingStatus as Status } from '@prisma/client'` on line 1 and reads `Status.CONFIRMED`
- **THEN** both files call `table bookingstatus` at line 1, and `src/renamed.ts` does not call `table status`, because the table is named as the package exports the name

#### Scenario: A model used only as a type names nothing
- **WHEN** `src/declared.ts` has `import type { Tenant } from '@prisma/client'` and `src/inline.ts` has `import { type Tenant, PrismaClient } from '@prisma/client'`
- **AND** `src/annotated.ts` has `import { Tenant } from '@prisma/client'` and uses `Tenant` only as a parameter type, a return type and a type argument
- **AND** `src/types.ts` has `import('@prisma/client').Tenant`, `import('@prisma/client').TenantBranch[]` and `typeof import('@prisma/client')`
- **THEN** none of the files calls a table

#### Scenario: A re-exported model names nothing
- **WHEN** `src/models.ts` has `export type { Tenant, Prisma } from '@prisma/client'`, `src/renamed.ts` has `export { Tenant as T } from '@prisma/client'` and `src/listed.ts` has `import { BookingStatus } from '@prisma/client'` and `export { BookingStatus }`
- **THEN** none of the files calls a table, because a re-export is not a use

#### Scenario: A namespace import of the client names nothing
- **WHEN** `src/client.ts` has `import * as client from '@prisma/client'`, a default import of it, `import { default as other } from '@prisma/client'`, `import '@prisma/client'`, `export * from '@prisma/client'`, `export * as models from '@prisma/client'` and `export { default as Client } from '@prisma/client'`
- **THEN** the file calls no table
