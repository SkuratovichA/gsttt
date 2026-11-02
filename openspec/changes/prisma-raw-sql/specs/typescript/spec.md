# Spec Delta

## ADDED Requirements

### Requirement: Raw SQL of the prisma pack
With the `prisma` pack, a template tagged `$queryRaw` or `$executeRaw`, the literal first argument of `$queryRawUnsafe` or `$executeRawUnsafe`, each on a receiver of up to three segments, and a template tagged `Prisma.sql`, SHALL be an embedded `sql` document at its line. A computed part of a template SHALL be written as `?` in the document.

#### Scenario: A raw query template is read as SQL with its computed parts written as placeholders
- **WHEN** `src/queries.ts` of a project that depends on `@prisma/client` has ``prisma.$queryRaw`SELECT id FROM a WHERE id = ${id}` `` on line 5 and, on line 8, ``prisma.$queryRaw`SELECT id FROM b WHERE id IN (${ids[0]}, ${ids[1]}) LIMIT ${limit}` ``
- **THEN** the file embeds a `sql` document at line 5 with the text `SELECT id FROM a WHERE id = ?`
- **AND** it embeds a `sql` document at line 8 with the text `SELECT id FROM b WHERE id IN (?, ?) LIMIT ?`
- **AND** the file has no unread site

#### Scenario: A raw query template with a type argument is read as SQL
- **WHEN** `src/queries.ts` has ``prisma.$queryRaw<{ id: string }[]>`SELECT id FROM c WHERE id = ${id}::uuid` `` on line 11 and ``prisma.$executeRaw`DELETE FROM d` `` on line 13
- **THEN** the file embeds a `sql` document at line 11 with the text `SELECT id FROM c WHERE id = ?::uuid`
- **AND** it embeds a `sql` document at line 13 with the text `DELETE FROM d`

#### Scenario: A raw query on a client held by this is read as SQL
- **WHEN** `src/service.ts` has ``this.prisma.$queryRaw`SELECT id FROM reports` `` on line 7 and ``this.db.client.$queryRaw`SELECT id FROM audits` `` on line 11
- **THEN** the file embeds a `sql` document at line 7 with the text `SELECT id FROM reports`
- **AND** it embeds a `sql` document at line 11 with the text `SELECT id FROM audits`

#### Scenario: An unsafe raw query with a literal is read as SQL
- **WHEN** `src/unsafe.ts` has `prisma.$queryRawUnsafe('SELECT id FROM a')` on line 5 and ``prisma.$executeRawUnsafe(`UPDATE b SET seen = true`)`` on line 7
- **THEN** the file embeds a `sql` document at line 5 with the text `SELECT id FROM a`
- **AND** it embeds a `sql` document at line 7 with the text `UPDATE b SET seen = true`

#### Scenario: A Prisma.sql template is read as SQL whole or in part
- **WHEN** `src/fragments.ts` has ``Prisma.sql`SELECT id FROM a WHERE id = ${id}` `` on line 3 and ``Prisma.sql`AND id = ${id}` `` on line 5
- **THEN** the file embeds a `sql` document at line 3 with the text `SELECT id FROM a WHERE id = ?`
- **AND** it embeds a `sql` document at line 5 with the text `AND id = ?`

#### Scenario: A Prisma.sql template passed to a raw query is read once
- **WHEN** `src/unsafe.ts` has ``prisma.$queryRaw(Prisma.sql`SELECT id FROM c WHERE id = ${id}`)`` on lines 11 and 12
- **THEN** the file embeds one `sql` document for it, at line 12, with the text `SELECT id FROM c WHERE id = ?`

#### Scenario: An unsafe raw query with a constant text is read as SQL
- **WHEN** `src/constant.ts` has `const TEXT = 'SELECT id FROM f'` and `prisma.$queryRawUnsafe(TEXT)` on line 7
- **THEN** the file embeds a `sql` document at line 7 with the text `SELECT id FROM f`

#### Scenario: A Prisma.sql template held in a name is read where it stands
- **WHEN** `src/held.ts` has ``const query = Prisma.sql`SELECT id FROM e` `` on line 5 and `prisma.$queryRaw(query)` on line 7
- **THEN** the file embeds one `sql` document, at line 5
- **AND** the file has no unread site

#### Scenario: A raw query is read only in a project that depends on the Prisma client
- **WHEN** `src/queries.ts` has ``prisma.$queryRaw`SELECT id FROM a` `` and the project does not depend on `@prisma/client`
- **THEN** the file embeds no document

### Requirement: Raw SQL built at run time
An `Unsafe` raw query whose text is built at run time, a template with a computed part or a variable that holds no constant, SHALL be an unread site at its line and embed no document. A `Prisma.sql` fragment that is no whole statement SHALL name no table.

#### Scenario: An unsafe raw query with a computed text is unread
- **WHEN** `src/unsafe.ts` has ``prisma.$queryRawUnsafe(`SELECT id FROM ${table}`)`` on line 9 and `prisma.$executeRawUnsafe(text)`, with `text` a parameter, on line 14
- **THEN** the file has an unread site at line 9 for `prisma.$queryRawUnsafe`, whose reason is that the text is not a literal
- **AND** it has one at line 14 for `prisma.$executeRawUnsafe`, with the same reason

#### Scenario: A Prisma.sql fragment that is no statement names no table
- **WHEN** a project has `model User` in its schema and a file with ``Prisma.sql`AND id = ${id}` ``
- **THEN** that file calls no table

### Requirement: One key for a table however a query names it
A table named by a raw query, by a model call and by a Prisma model SHALL be one contract key, in lower case, whether the query quotes the name or qualifies it with a schema.

#### Scenario: A raw query, a model call and a Prisma model name one table
- **WHEN** `prisma/schema.prisma` has `model User`, `src/model.ts` calls `prisma.user.findMany()`, `src/quoted.ts` has ``prisma.$queryRaw`SELECT id FROM "User" WHERE id = ${id}` `` and `src/qualified.ts` has ``prisma.$queryRaw`SELECT id FROM public."User"` ``
- **THEN** `table user` is declared by the schema alone
- **AND** it is called by `src/model.ts` through `prisma`, and by `src/qualified.ts` and `src/quoted.ts` through `sql`

## MODIFIED Requirements

### Requirement: Packs that find documents in the provides
The provides of a project SHALL name every pack whose `[[embeds]]` rules find a document, in `via` of the calls of the contract that document holds.

#### Scenario: The graphql pack is credited with the documents it finds
- **WHEN** a project that depends on `graphql-tag` has a file with a `gql` tagged template
- **THEN** the provides name `graphql` in `via` of the `graphql` calls

#### Scenario: The prisma pack is credited with the SQL it finds
- **WHEN** a project that depends on `@prisma/client` has a file with a `$queryRaw` tagged template
- **THEN** the provides name `prisma` in `via` of the `table` calls
