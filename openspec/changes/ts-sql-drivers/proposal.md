# Proposal: Read the SQL of the common drivers

## Why

SQL that TypeScript sends through a driver is not read: `db.query('SELECT id FROM users')` on a `Pool` of `pg` names no table, so the question of who touches a table misses the most direct code there is. Python reads the same thing through its `dbapi` pack, and the `prisma` pack reads raw SQL the same way, so this needs a pack and no new code.

## What Changes

- TypeScript: a new built-in pack `sql-drivers` embeds the literal or template first argument of `query`, `execute`, `prepare` and `exec` on a receiver of up to three segments as a `sql` document, a computed part of a template written as `?`. It applies to a project that depends on `pg`, `mysql`, `mysql2`, `better-sqlite3`, `sqlite3` or `@neondatabase/serverless`.
- A first argument with no text at all records nothing and is no unread site, because a method of these names may hold no SQL.
- Core: an `[[embeds]]` rule takes an optional `computed`, `unread` (the default, so the rules of the `prisma` and `dbapi` packs are unchanged) or `placeholder`, which reads a text with a computed part with the part written as the placeholder of the language. The JSON Schema of a pack lists it.
- The blind spots of `table` calls say what the drivers do not read.
- Versions: none move. The packs are part of the salt of the project, so `READER_VERSION` stays, and a rule with no `computed` gives what it gave, so `FACT_FORMAT` stays.

## Impact

- Capability `typescript`.
- `crates/lang-ts/packs/sql-drivers.toml` and its registration in `crates/lang-ts/src/connector.rs`; `EmbedRule` in `crates/core/src/rules.rs`; the blind spots in `crates/lang-ts/src/provides.rs`.
- A project that depends on one of these drivers gains `table` calls and `sql` documents where its code passes a literal to one of these methods; any other project gives the same index as before.
