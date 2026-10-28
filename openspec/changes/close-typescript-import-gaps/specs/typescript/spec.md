# Spec Delta

## ADDED Requirements

### Requirement: Imports in types
The connector SHALL read `typeof import('x')` and `import('x').T` in a type as a static import of `x` flagged type-only, at the line of the specifier. `typeof import('x')` SHALL name `*`, as a namespace import does, and `import('x').A.B` and `typeof import('x').A` SHALL name `A`, the first name of the qualifier.

#### Scenario: A type query of a module is a type-only import
- **WHEN** `src/types.ts` has `export type Lib = typeof import('./lib')` on line 1 and `export type Helper = typeof import('./lib').helper` on line 2, and `src/lib.ts` exists
- **THEN** `src/types.ts` has two static imports of `./lib`, both flagged type-only, the first at line 1 naming `*` and the second at line 2 naming `helper`
- **AND** both resolve to `src/lib.ts`

#### Scenario: An import type names the first segment of its qualifier
- **WHEN** `src/types.ts` has `export type Editor = import('./lib').Editor` on line 1 and `export type Inner = import('./lib').Outer.Inner` on line 2
- **THEN** the file has static imports of `./lib` flagged type-only, naming `Editor` at line 1 and `Outer` at line 2

#### Scenario: A nested import type is read at the line of its specifier
- **WHEN** `src/types.ts` has `typeof import(` on line 1 and `'./lib'` on line 2, `import('./lib').Box<typeof import('./other')>` on line 4 and `Promise<typeof import('./lib')>` on line 5
- **THEN** the file has type-only static imports of `./lib` at lines 2 and 5 naming `*`, and at line 4 one of `./lib` naming `Box` and one of `./other` naming `*`

### Requirement: Mocked modules
The connector SHALL read the first argument of `vi.mock`, `vi.doMock`, `vi.unmock`, `vi.doUnmock`, `vi.importActual`, `vi.importMock`, `jest.mock`, `jest.doMock`, `jest.unmock`, `jest.dontMock`, `jest.requireActual` and `jest.requireMock` as a dynamic import with no names when it is a string or a template with no expression. A computed first argument SHALL be neither an import nor an unread site.

#### Scenario: A mocked or actual module is a dynamic import
- **WHEN** `src/runners.test.ts` has `'./lib'` as the first argument of each of the twelve calls above, one per line, and `` vi.mock(`./lib`) `` on line 13
- **THEN** each of the thirteen calls is a dynamic import of `./lib` at its line, with no names, resolving to `src/lib.ts`

#### Scenario: A module named at run time is no import
- **WHEN** `src/computed.test.ts` has `vi.mock(path)`, `` jest.mock(`./${name}`) ``, `vi.importActual('./' + name)` and `await import(name)`
- **THEN** the file has no imports
- **AND** its only unread site is the one of `await import(name)`, with the reason computed at run time

#### Scenario: A mock call on another receiver is no import
- **WHEN** `src/others.test.ts` has `server.mock('./lib')`, `vi.fn('./lib')` and `mocker.importActual('./lib')`, and `src/lib.ts` exists
- **THEN** the file has no imports

#### Scenario: A mock that asks for the actual module reaches it
- **WHEN** `src/lib.test.ts` has `vi.mock('./lib', async () => ({ ...(await vi.importActual<typeof import('./lib')>('./lib')) }))` over lines 1 to 3 and `vi.mock('./editor', async (importOriginal) => ({ ...(await importOriginal<typeof import('./editor')>()) }))` over lines 5 to 7
- **THEN** the file imports `./lib` dynamically at lines 1 and 2 and as a type at line 2, and `./editor` dynamically at line 5 and as a type at line 6
- **AND** the imports resolve to `src/lib.ts` and `src/editor.ts`, so a change to either module reaches the test

### Requirement: Query suffixes
A specifier with a query, such as `./terms.md?raw`, `./logo.svg?url` or `ui-kit/theme.css?inline`, SHALL resolve as the same specifier without the query does, and the import SHALL keep the specifier as written.

#### Scenario: A query does not change where an import leads
- **WHEN** `src/page.ts` imports `../docs/terms.md?raw`, `./worker?worker`, `./logo.svg?url`, `ui-kit/theme.css?inline`, `../generated/schema.json?url` and `./absent.md?raw`, and each of them again without its query, where `docs/terms.md`, `src/worker.ts` and `src/logo.svg` exist, `ui-kit` is declared in `package.json`, `generated/schema.json` exists on disk but an ignore file keeps it out of the file list, and `absent.md` does not exist
- **THEN** each import with a query leads where the same specifier without the query leads
- **AND** the first three resolve to `docs/terms.md`, `src/worker.ts` and `src/logo.svg`, the fourth to the package `ui-kit`, the fifth is unresolved as ignored and the sixth as not found
- **AND** the facts keep the specifiers with their queries

### Requirement: Aliases of this
A chain rule SHALL read a chain that starts at a name bound by `const <name> = this` or `let <name> = this`, whatever the name, as a chain from `this`. The alias SHALL hold for the whole file, and a name that the file binds to anything else anywhere in it, as a variable, a parameter, a function, a class or an import, SHALL be no alias.

#### Scenario: An alias of this reaches the Prisma client
- **WHEN** `src/tenants.ts` has a class with the field `prisma = new PrismaClient()`, a method that binds `const self = this` and calls `self.prisma.tenant.findUnique({ where: { id } })` on line 9, a method that binds `let that = this` and calls `that.prisma.booking.count()` on line 15, and a function that binds `const holder = shared` and calls `holder.prisma.invoice.findMany()`
- **THEN** the file calls `table tenant` at line 9 and `table booking` at line 15
- **AND** it calls no other table

#### Scenario: A name bound to anything else is no alias of this
- **WHEN** `src/self-alias.ts` has a class with the field `prisma = new PrismaClient()` and a method that binds `const self = this` and calls `self.prisma.tenant.findFirst()`, and a function with the parameter `self: Mock` that calls `self.prisma.tenant.findMany()`
- **AND** `src/consistent.ts` has a class with the field `prisma = new PrismaClient()`, a method that binds `const self = this` and calls `self.prisma.tenant.findFirst()` on line 8, and a method that binds `let self = this` and calls `self.prisma.booking.count()` on line 13
- **THEN** `src/self-alias.ts` calls no table
- **AND** `src/consistent.ts` calls `table tenant` at line 8 and `table booking` at line 13

### Requirement: Roots by type
A chain rule MAY list root types. A parameter, a variable or a class property declared with one of them, alone, in a union or as the first type argument of `Pick`, `Omit`, `Partial`, `Required`, `Readonly` or `NonNullable`, SHALL then start a chain of the rule, and a constructor parameter with a modifier SHALL start one as `this.<name>` too. A name or a property that the file also declares anywhere in it with another type, or with none, SHALL start no chain. The `prisma` pack lists `PrismaClient`, `PrismaService` and `Prisma.TransactionClient`.

#### Scenario: A parameter typed as a pick of the client is a root
- **WHEN** `src/tenants.ts` has `import type { Prisma } from '@prisma/client'`, a function with the parameter `reader: Pick<Prisma.TransactionClient, 'tenant' | 'tenantBranch'>` that calls `reader.tenant.findUnique()` on line 4 and `reader.tenantBranch.findMany()` on line 5, and a function with the parameter `options: { tenant: { findFirst(): void } }` that calls `options.tenant.findFirst()`, and `src/tenants.test.ts` has `const mockPrisma = { tenant: { findFirst: vi.fn() } }` and calls `mockPrisma.tenant.findFirst()`
- **THEN** `src/tenants.ts` calls `table tenant` at line 4 and `table tenantbranch` at line 5, and no other table
- **AND** `src/tenants.test.ts` calls no table

#### Scenario: A client typed as PrismaClient or PrismaService is a root
- **WHEN** `src/invoices.ts` has a class with the property `private readonly client: PrismaClient` and the constructor `constructor(private readonly store: PrismaService, plain: PrismaService)`, whose methods call `this.store.invoice.findMany()` on line 12, `this.client.payment.findUnique()` on line 16 and `this.plain.refund.findMany()` on line 20, and a function with the parameter `client: PrismaClient | null` that calls `client.booking.count()` on line 25
- **THEN** the file calls `table invoice` at line 12, `table payment` at line 16 and `table booking` at line 25
- **AND** the call on `this.plain` names no table, because a constructor parameter without a modifier is no property

#### Scenario: A name declared with another type is no root
- **WHEN** `src/names.ts` has a function with the parameter `tx2: Pick<Prisma.TransactionClient, 'tenant'>` and a function with the parameter `tx2: { tenant: { findFirst(): void } }`, and both call `tx2.tenant.findFirst()`
- **AND** `src/properties.ts` has a class with the property `client: PrismaClient` and a class with the property `client: { tenant: { findFirst(): void } }`, and a method of each calls `this.client.tenant.findFirst()`
- **AND** `src/consistent.ts` has a function with the parameter `client: PrismaClient` that calls `client.tenant.findFirst()` on line 4 and a function with the parameter `client: PrismaClient | null` that calls `client.booking.count()` on line 8
- **THEN** `src/names.ts` and `src/properties.ts` call no table
- **AND** `src/consistent.ts` calls `table tenant` at line 4 and `table booking` at line 8

### Requirement: Signatures bind nothing
A parameter of a signature without a body SHALL bind no name, so that it is neither a root nor a reason for another declaration of its name to be no root. These are the method, call and construct signatures of an interface or a type literal, a function type, a constructor type, an overload, an abstract method and a `declare function`.

#### Scenario: A signature without a body binds nothing
- **WHEN** `src/signatures.ts` has a parameter typed `PrismaClient`, each with another name, in an interface method signature, a call signature, a construct signature, a function type, a `declare function`, an overload signature and an abstract method, and calls `.tenant.findFirst()` on each of those seven names, none of which the file declares anywhere else
- **THEN** the file calls no table

### Requirement: Type aliases of root types
A `type` alias of the same file SHALL be read as the type it defines, through any number of aliases, when a declaration is checked for a root type. An alias that is defined by itself, directly or through other aliases, SHALL define no root type, and no alias SHALL be followed more than once for a declaration, so that a type that names itself many times is read as fast as one that names itself once.

#### Scenario: A local type alias of a client type is a root
- **WHEN** `src/readers.ts` has `type Client = Prisma.TransactionClient` on line 3, `type Reader = Pick<Client, 'tenant'>` on line 4, `type Options = { tenant: { findFirst(): void } }` on line 5, and `type Loop = Again` and `type Again = Loop` on lines 6 and 7, and functions that take `reader: Reader`, `options: Options` and `loop: Loop` and call `reader.tenant.findUnique()` on line 10, `options.tenant.findFirst()` on line 14 and `loop.tenant.findMany()` on line 18
- **THEN** the file calls `table tenant` at line 10 and no other table

#### Scenario: A long chain of type aliases is followed to its end
- **WHEN** `src/chain.ts` has the nine aliases `Chain1` to `Chain9`, where `Chain1` is `PrismaClient` and each of the others is the one before it, and a function with the parameter `client: Chain9` that calls `client.tenant.findFirst()` on line 14
- **AND** `src/cycle.ts` has `type Ping = Pong` and `type Pong = Ping` and a function with the parameter `ring: Ping` that calls `ring.tenant.findMany()`, and `type Back = Forth | PrismaClient` and `type Forth = Back` and a function with the parameter `value: Forth` that calls `value.booking.count()` on line 13
- **AND** `src/fan-out.ts` has `type Wide = Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide | Wide` and a function with the parameter `value: Wide` that calls `value.tenant.findFirst()`
- **THEN** `src/chain.ts` calls `table tenant` at line 14 and `src/cycle.ts` calls `table booking` at line 13, and no other table
- **AND** `src/fan-out.ts` calls no table, and the project is indexed without waiting on `Wide`

### Requirement: Local types shadow root types
A name that the file declares itself, as a `type` alias, an `interface`, a `class` or an `enum`, SHALL be that local type and never the root type of a pack with the same name. A local `type` alias SHALL be read as the type it defines, which may be a root type, and an `interface`, a `class` or an `enum` SHALL be no root type. Only a name that the file does not declare SHALL be checked against the root types. A root type imported under another name, a root type reached through a namespace import under another name, and an intersection with a root type SHALL NOT be read as a root type.

#### Scenario: A local type shadows a root type
- **WHEN** `src/alias.ts` has `type PrismaClient = { tenant: { findFirst(): void } }`, `src/interface.ts` has an `interface PrismaClient`, `src/class.ts` has a `class PrismaService`, `src/enum.ts` has an `enum PrismaClient`, and each of the four has a function with a parameter of that type that calls `.tenant.findFirst()` on it
- **AND** `src/alias-of-root.ts` has `import type { Prisma } from '@prisma/client'` and `type PrismaClient = Pick<Prisma.TransactionClient, 'tenant'>`, and a function with the parameter `client: PrismaClient` that calls `client.tenant.findFirst()` on line 6
- **THEN** `src/alias-of-root.ts` calls `table tenant` at line 6
- **AND** the other four files call no table

#### Scenario: A renamed or combined client type is no root
- **WHEN** `src/renamed.ts` has `import type { PrismaClient as Base } from '@prisma/client'` and a parameter `client: Base`, `src/namespaced.ts` has `import type { Prisma as P } from '@prisma/client'` and a parameter `reader: P.TransactionClient`, and `src/intersected.ts` has `import type { PrismaClient } from '@prisma/client'` and a parameter `client: PrismaClient & { extra: true }`, each of them calling `.tenant.findFirst()` on its parameter
- **THEN** the three files call no table

### Requirement: Imported model types
A pack MAY name a contract for each name a file takes from a package. With the `prisma` pack, a name imported or re-exported by name from `@prisma/client` SHALL call the table of that name at the line of the import, as the package exports it and not as the file renames it, except `Prisma`, `PrismaClient`, `PrismaPromise` and `$Enums`. A namespace import, a default import, a side-effect import and `export *` SHALL name nothing. An import type `import('@prisma/client').Tenant` SHALL be read as the import of the first name of its qualifier is, at the line of its specifier, and one without a qualifier SHALL name nothing. Only a name taken from a package that `from` lists SHALL be read, so the model types of a client generated to a custom `output` path, or of a package that re-exports the client with `export * from '@prisma/client'`, SHALL name no table. An enum imported by name SHALL be read as a table of that name, because the import does not say which names are models.

#### Scenario: An imported model type calls its table
- **WHEN** `src/models.ts` has `import type { Tenant, TenantBranch as Branch, Prisma, PrismaClient } from '@prisma/client'` over lines 1 to 6 and `import { BookingStatus } from '@prisma/client'` on line 7
- **THEN** the file calls `table tenant` and `table tenantbranch` at line 1
- **AND** `Prisma` and `PrismaClient` name no table
- **AND** the file calls `table bookingstatus` at line 7, because the import does not say whether a name is a model or an enum

#### Scenario: A re-exported model type calls its table
- **WHEN** `src/models.ts` has `export type { Tenant, Prisma } from '@prisma/client'` on line 1
- **AND** `src/renamed.ts` has `export { Tenant as T } from '@prisma/client'` on line 1
- **THEN** `src/models.ts` calls `table tenant` at line 1 and no other table
- **AND** `src/renamed.ts` calls `table tenant` at line 1 and not `table t`, because the table is named as the package exports the model and not as the file renames it

#### Scenario: An import type reaches the import rules
- **WHEN** `src/types.ts` has `import('@prisma/client').Tenant` on line 1, `import('@prisma/client').TenantBranch[]` on line 2, `import('@prisma/client').Prisma.TenantWhereInput`, `import('@prisma/client').PrismaClient`, `typeof import('@prisma/client')` and `import('ui-kit').Tenant`
- **AND** `src/declared.ts` has `import type { Tenant } from '@prisma/client'` on line 1
- **THEN** `src/types.ts` calls `table tenant` at line 1 and `table tenantbranch` at line 2 and no other table
- **AND** `src/declared.ts` calls `table tenant` at line 1, as the import type does

#### Scenario: A namespace import of the client names nothing
- **WHEN** `src/client.ts` has `import * as client from '@prisma/client'`, a default import of it, `import { default as other } from '@prisma/client'`, `import '@prisma/client'`, `export * from '@prisma/client'`, `export * as models from '@prisma/client'` and `export { default as Client } from '@prisma/client'`
- **THEN** the file calls no table
