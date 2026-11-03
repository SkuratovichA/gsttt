# Proposal: Read a computed part of Python SQL as a placeholder

## Why

`cursor.execute(f"SELECT id FROM bookings WHERE id = {task}")` names the table `bookings` plainly, yet it is an unread site with no table, because the f-string has a computed part. The TypeScript driver pack reads the same query with the computed part written as the SQL placeholder `?`. Python should read it the same way.

## What Changes

- The `dbapi` pack and the `sqlalchemy` pack, for `text()`, set `computed = "placeholder"` on their `[[embeds]]`: an f-string, a `+` that joins a literal and a name, and a `.format` template embed an `sql` document with `?` for each computed part, so a table that is plainly named is called.
- A string with `%` on it is not changed: it stays an unread site, as does an argument the connector reads as no text at all.
- A statement whose table or whole clause is computed, `SELECT id FROM ?`, does not parse and names no table. It is no unread site. The provides of every Python project name it as a blind spot of its tables, in the words of the TypeScript driver pack.
- Versions: none. The packs feed the key of the facts of a file, so the facts of the Python files are read again.

## Impact

- Capability `python`: the requirement "DB-API arguments that are not literals" is replaced by "DB-API arguments built at run time", because its scenarios about a SQL string built at run time and a format template stop being true and a scenario of a modified requirement cannot be renamed.
- `crates/lang-python/packs/dbapi.toml`, `crates/lang-python/packs/sqlalchemy.toml` and `DBAPI_GAPS` in `crates/lang-python/src/provides.rs`.
- A Python project that builds SQL with an f-string, `+` or `.format` gains the tables those statements name; a project that does not, gives the same answers.
