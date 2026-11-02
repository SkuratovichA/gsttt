## ADDED Requirements

### Requirement: SQL migrations
In a `.sql` file whose path matches the `sql` connector's `migrations` globs, every table the file names SHALL be declared and none called, once, at the first line that names it. The default is `**/migrations/**` and `**/migration/**`; an empty list matches no file; an invalid glob SHALL be an options error naming `migrations` and the glob.

#### Scenario: A migration declares every table it names
- **WHEN** `prisma/migrations/20240101000000_init/migration.sql` has `CREATE TABLE "Tenant"`, `INSERT INTO "Tenant"`, `UPDATE "Branch"` and `ALTER TABLE "Tenant"`
- **THEN** it declares `table tenant` once, at the `CREATE` line, although the `ALTER TABLE` names it again, and `table branch`, and calls no table

#### Scenario: The singular migration directory is a migration
- **WHEN** `db/migration/V1__seed.sql` has `INSERT INTO seed_rows`
- **THEN** it declares `table seed_rows`

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

## MODIFIED Requirements

### Requirement: SQL
`CREATE`, `ALTER` and `DROP` of a table or view, and `CREATE INDEX … ON`, SHALL declare `table <name>`; in a file that is not a migration, `FROM`, `JOIN`, `INSERT INTO`, `UPDATE`, `DELETE FROM` and `MERGE INTO` SHALL call `table <name>`, leaving out the names of common table expressions. Names SHALL drop their schema and quotes. `ALTER TABLE <name> RENAME TO <new>` SHALL declare the new name as well.

#### Scenario: A migration declares its tables
- **WHEN** a file has `CREATE TABLE IF NOT EXISTS public."BookingItem" (id int);` and `ALTER TABLE bookings ADD COLUMN note text;`
- **THEN** it declares `table bookingitem` and `table bookings`

#### Scenario: A rename declares the new name
- **WHEN** a file has `ALTER TABLE bookings RENAME TO reservations;`
- **THEN** it declares `table bookings` and `table reservations`

#### Scenario: Queries call their tables
- **WHEN** a file has `WITH recent AS (SELECT * FROM bookings) SELECT * FROM recent JOIN booking_items i ON true WHERE id = %s;`
- **THEN** it calls `table bookings` and `table booking_items` and not `table recent`

### Requirement: Embedded SQL
The connector SHALL read embedded `sql` documents the same way as it reads `sql` files that are not migrations, whatever the path of the host file, and SHALL return lines within the document, line 1 being its first line; the engine places them in the host file.

#### Scenario: Embedded SQL reports lines within the document
- **WHEN** `parse_embedded` gets `SELECT id FROM bookings` embedded at line 7
- **THEN** it returns a call to `table bookings` at line 1

#### Scenario: SQL embedded in a file under migrations calls its tables
- **WHEN** a file under `migrations/` embeds `INSERT INTO bookings (id) VALUES (1)`
- **THEN** that file calls `table bookings`
