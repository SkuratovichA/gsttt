# Spec Delta

## ADDED Requirements

### Requirement: Raw SQL of the SQL drivers
With the `sql-drivers` pack, the literal or template first argument of `query`, `execute`, `prepare` and `exec`, on a receiver of up to three segments, SHALL be an embedded `sql` document at its line, a computed part as `?`. A first argument with no text at all SHALL record nothing and SHALL NOT be an unread site, as a method of that name may hold no SQL. The pack SHALL apply to a project that depends on `pg`, `mysql`, `mysql2`, `better-sqlite3`, `sqlite3` or `@neondatabase/serverless`.

#### Scenario: A driver query is read as SQL
- **WHEN** `src/users.ts` has `db.query('SELECT id FROM users WHERE id = $1', [id])` on line 4 and `this.pool.query('DELETE FROM sessions')` on line 8
- **THEN** the file embeds a `sql` document at line 4 with the text `SELECT id FROM users WHERE id = $1` and one at line 8 with the text `DELETE FROM sessions`

#### Scenario: A driver query calls the tables it names
- **WHEN** a project that depends on `pg` has `db.query('SELECT id FROM users')` in one file and `pool.query('DELETE FROM sessions')` in another
- **THEN** each file calls its table, `users` and `sessions`

#### Scenario: A query kept in a constant is read
- **WHEN** `src/constant.ts` has `const LIST = 'SELECT id FROM accounts'` and `db.query(LIST)`
- **THEN** the file embeds a `sql` document with that text at the line of the call

#### Scenario: A query template is read with its computed parts as placeholders
- **WHEN** `src/template.ts` has ``pool.query(`SELECT id FROM orders WHERE id = ${id}`)`` on line 3 and ``pool.query(`SELECT id FROM ${table}`)`` on line 5
- **THEN** the file embeds the `sql` documents `SELECT id FROM orders WHERE id = ?` at line 3 and `SELECT id FROM ?` at line 5
- **AND** it has no unread site

#### Scenario: A computed table names no table
- **WHEN** a file has ``pool.query(`SELECT id FROM ${table}`)``
- **THEN** the file calls no table and has no unread site

#### Scenario: A call with no text in its first argument records nothing
- **WHEN** `src/silent.ts` has `client.query({ query: GET_USERS })` and `pool.query(text)` with `text` a parameter
- **THEN** the file embeds no document and has no unread site

#### Scenario: A literal that is no SQL names no table
- **WHEN** a project has `cp.exec('ls -la')` beside files that call the tables `users`, `sessions` and `orders`
- **THEN** the tables of the project are those three and no more
- **AND** the file with `cp.exec` calls none and has no unread site

#### Scenario: A project without a SQL driver reads no query
- **WHEN** a project that depends on no driver has `db.query('SELECT id FROM users')`
- **THEN** the file embeds no document

#### Scenario: The sql-drivers pack is credited with the tables it finds
- **WHEN** a project that depends on `pg` has a file that calls `db.query('SELECT id FROM users')`
- **THEN** the provides name `sql-drivers` in `via` of the `table` calls
