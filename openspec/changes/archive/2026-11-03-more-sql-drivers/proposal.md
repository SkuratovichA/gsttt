# Proposal: Read the SQL of more drivers, objects, tags and calls on a value

## Why

The `sql-drivers` pack turns on for six packages and reads one form of call. A project on `mssql`, `@libsql/client`, `@planetscale/database`, `oracledb`, `sequelize`, `postgres` or `@vercel/postgres` gets no tables from its raw SQL. Three common forms are read on no driver: a query held in an object (`client.execute({ sql, args })`, `pool.query({ text, values })`), a template tagged `sql`, and a call on a value no name stands for (`pool.request().query('…')`, `new sql.Request().query('…')`, `getDb().query('…')`).

## What Changes

- The pack also detects `mssql`, `@libsql/client`, `@planetscale/database`, `oracledb`, `sequelize`, `postgres` and `@vercel/postgres`.
- The `sql` and the `text` property of an object given as the first argument are read like a literal first argument. A call that carries no such document is no site of the rule, so `client.query({ query: GET_USERS })` and `pool.query(text)` stay silent, and `pool.query({ text: build() })` is an unread site. `EmbedRule::carried_by` in `gsttt-core` tells the two, and the TypeScript connector uses it for calls and for tagged templates.
- A template tagged `sql`, `x.sql` or `sql.unsafe` is read. A fragment that is no statement, such as Drizzle's ``sql`lower(${x})` ``, parses to nothing and costs nothing.
- A call on a value no name stands for, whose callee is a static member of a call, a `new` or an `await`, is read against the embed rules whose callee names no receiver, as `().query`. A regular expression `exec` is no such value.
- The blind spot of the pack and its first line say what is read now.
- The SQL parser reads `@id` and `:id` as it does `?` and `$1`; the dialect is not changed.
- Versions: `READER_VERSION` of the TypeScript connector goes from 28 to 29, because the facts of the same text change. No `FACT_FORMAT` or `LAYOUT` change: no stored shape changes.

## Impact

- Capability `typescript`.
- `crates/lang-ts/packs/sql-drivers.toml`, `EmbedRule::carried_by` in `crates/core/src/rules.rs`, `embeds` and `visit_call_expression` in `crates/lang-ts/src/analyze.rs`, `on_a_value` in `crates/lang-ts/src/shape.rs`, `SQL_DRIVERS_BLIND_SPOT` in `crates/lang-ts/src/provides.rs`.
- A call of `$queryRawUnsafe` with no argument at all, which was an unread site, is now no site, since it carries no document.
