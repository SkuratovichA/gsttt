## ADDED Requirements

### Requirement: SQL migrations
In a `.sql` file whose path matches the `sql` connector's `migrations` globs, every table the file names SHALL be declared and none called, once, at the first line that names it. The default is `**/migrations/**` and `**/migration/**`; an empty list matches no file; an invalid glob SHALL be an options error naming `migrations` and the glob. SQL embedded in code SHALL be read as queries.

#### Scenario: A migration declares every table it names
- **WHEN** `prisma/migrations/20240101000000_init/migration.sql` has `CREATE TABLE "Tenant"`, `INSERT INTO "Tenant"` and `UPDATE "Branch"`
- **THEN** it declares `table tenant` once, at the `CREATE` line, and `table branch`, and calls no table

#### Scenario: A query outside the migrations calls its tables
- **WHEN** `src/queries/report.sql` has `SELECT id FROM tenant`
- **THEN** it calls `table tenant`

#### Scenario: The migrations option replaces the default globs
- **WHEN** `migrations` is `["db/schema/**"]`
- **THEN** a file under `db/schema/` declares its tables and a file under `prisma/migrations/` calls them again

#### Scenario: An empty migrations list reads no migration
- **WHEN** `migrations` is `[]`
- **THEN** a file under `prisma/migrations/` calls the tables its queries name

#### Scenario: An invalid migrations glob is an options error
- **WHEN** `migrations` holds `[`
- **THEN** opening the `sql` connector fails with an options error that names `migrations` and `[`
