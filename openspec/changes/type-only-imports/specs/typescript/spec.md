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
- **AND** `src/types.ts` has `import('@prisma/client').Tenant` on line 1, `import('@prisma/client').TenantBranch[]`, `import('@prisma/client').Prisma.TenantWhereInput` on line 3 and `typeof import('@prisma/client')`
- **THEN** none of the files calls a table

#### Scenario: A re-exported model names nothing
- **WHEN** `src/models.ts` has `export type { Tenant, Prisma } from '@prisma/client'`, `src/renamed.ts` has `export { Tenant as T } from '@prisma/client'` and `src/listed.ts` has `import { BookingStatus } from '@prisma/client'` and `export { BookingStatus }`
- **THEN** none of the files calls a table, because a re-export is not a use

#### Scenario: A namespace import of the client names nothing
- **WHEN** `src/client.ts` has `import * as client from '@prisma/client'`, a default import of it, `import { default as other } from '@prisma/client'`, `import '@prisma/client'`, `export * from '@prisma/client'`, `export * as models from '@prisma/client'` and `export { default as Client } from '@prisma/client'`
- **THEN** the file calls no table

### Requirement: Uses of an imported name as a value
A file uses an imported name as a value when its code names the local name outside every type. A name written in a type is no use of the value, and neither is a re-export, `import type`, an import type, or a field typed with the name, so `this.field` is no use of it.

#### Scenario: A name in a value position is a use
- **WHEN** a file imports `Tenant` from `@prisma/client` and has `Tenant()`, `[Tenant]`, `Tenant.ACTIVE`, `typeof Tenant` as an expression, `export default Tenant`, `class Row extends Tenant`, `enum Kind { A = Tenant.A }`, `Tenant as unknown`, `Tenant satisfies object`, `Tenant!`, `{ Tenant }`, `{ ...Tenant }`, `<Tenant />`, `(t = Tenant) => t`, a class property `x = Tenant`, `import Alias = Tenant.Inner`, or `Tenant.fields` in a method beside a parameter typed `Map<Box<string>, Tenant>`
- **THEN** each file calls `table tenant`

#### Scenario: A name outside a value position is no use
- **WHEN** a file imports `Tenant` from `@prisma/client` and uses it as a parameter or return type, in `implements Tenant`, in `interface Row extends Tenant`, as the type argument `g<Tenant>()`, in `typeof Tenant` written in a type, in `Tenant[]`, in the types of `satisfies` and `as`, in an ambient `declare function` signature, as the type of a constructor parameter property read as `this.tenant`, in `export { Tenant }` or `export { Tenant as T }`
- **AND** a file has `import type { Tenant }`, `import { type Tenant }`, `export type Row = import('@prisma/client').Tenant`, `export { Tenant as T } from '@prisma/client'` or `export type { Tenant } from '@prisma/client'`
- **THEN** none of the files calls a table
