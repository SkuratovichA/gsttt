# Proposal: Read a migration as a declaration of its tables

## Why

A migration changes the schema; it does not use it. Its `INSERT` and `UPDATE` statements and the queries inside its views made it a caller of the tables it touches, so every `prisma/migrations/*/migration.sql` counted as a user of the tables it created. Measured on two real repositories, migration files were a large share of the files falsely counted as users of a table, and a change to a table listed every migration that ever touched it.

## What Changes

- In a `.sql` file whose repository path matches the globs of the new `migrations` option of the `sql` connector, every table the file names is declared and none is called: each call becomes a declaration of the same table, and the file declares each table once, at the first line that names it.
- Without the option the globs are `**/migrations/**` and `**/migration/**`, which cover the layouts of Prisma, Flyway, golang-migrate, sqlx, Supabase and dbmate. An empty list reads no file as a migration. An invalid glob is an options error that names `migrations` and the glob.
- SQL embedded in code is always read as queries: the entry point that reads it is not given the path of the host file.
- `gsttt-core` gets one strict glob-set builder, `gsttt_core::glob::glob_set`, which the `openapi` connector, the external connectors of the config crate and the new option use. Their messages do not change. The tsconfig globs of `lang-ts` stay as they are, because tsc globs mean something else: a separator-aware `*`, a directory form, and an invalid pattern that lists nothing.
- The `version` of the `sql` connector goes from 1 to 2, because the same text now gives other facts. `FACT_FORMAT` and `LAYOUT` do not change: the glob helper changes no outcome in core, and no archived type changes shape.

## Impact

- Capability `contract-formats`.
- `crates/core/src/glob.rs`, `crates/contracts/src/sql.rs`, `crates/contracts/src/openapi.rs`, `crates/config/src/external.rs`, the SQL tests and fixtures.
- The version bump reads every `.sql` file again once.
- The requirements `SQL` and `Embedded SQL` say that the calls are made by a file that is not a migration, and that embedded SQL is read that way whatever the host file.
