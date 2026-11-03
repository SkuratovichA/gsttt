# Spec Delta

## ADDED Requirements

### Requirement: Raw SQL given to a driver in an object
With the `sql-drivers` pack, the `sql` or `text` property of an object literal that is the first argument of `query`, `execute`, `prepare` or `exec` SHALL be an embedded `sql` document, as a literal or template first argument is. An object with neither property, and a first argument that is no object literal, SHALL record nothing. A property that holds no text SHALL be an unread site.

#### Scenario: A query held in an object is read
- **WHEN** a file has `pool.query({ text: 'SELECT id FROM invoices', values: [1] })` on line 4, and another has `client.execute('SELECT id FROM notes')` on line 3 and `client.execute({ sql: 'SELECT id FROM tags WHERE id = ?', args: [id] })` on line 5
- **THEN** the first file embeds `SELECT id FROM invoices` at line 4
- **AND** the other embeds `SELECT id FROM notes` at line 3 and `SELECT id FROM tags WHERE id = ?` at line 5

#### Scenario: A query in an object that is not text is an unread site
- **WHEN** a file has `pool.query({ text: build() })`
- **THEN** the file has an unread site at the call and embeds no document

### Requirement: Raw SQL of a template tagged sql and of a call on a value
With the `sql-drivers` pack, a template tagged `sql`, `x.sql` or `sql.unsafe` SHALL be an embedded `sql` document, and so SHALL the first argument of `query`, `execute`, `prepare` or `exec` called on a call, a `new` or an `await`, such as `pool.request().query(…)`. A template that is no statement SHALL record no table. A regular expression `exec` SHALL record nothing.

#### Scenario: A template tagged sql is read
- **WHEN** a project that depends on `postgres` has ``sql`SELECT id FROM users WHERE id = ${id}` `` on line 3, and one that depends on `@vercel/postgres` has ``sql`SELECT id FROM users` `` on line 4 and ``db.sql`SELECT id FROM posts` `` on line 6
- **THEN** the first embeds `SELECT id FROM users WHERE id = ?` at line 3
- **AND** the second embeds `SELECT id FROM users` at line 4 and `SELECT id FROM posts` at line 6

#### Scenario: A fragment tagged sql that is no statement records nothing
- **WHEN** a file has ``sql`lower(${x})` ``
- **THEN** the file calls no table and has no unread site

#### Scenario: A call on a value no name stands for is read
- **WHEN** a file has `getDb().query('SELECT id FROM audits')` on line 3, and a project that depends on `mssql` has `pool.request().input('id', sql.Int, id).query('SELECT id FROM orders WHERE id = @id')` on line 5, `new sql.Request().query('SELECT id FROM refunds')` on line 7 and ``sql.query`SELECT id FROM carts WHERE id = ${id}` `` on line 9
- **THEN** the first file embeds `SELECT id FROM audits` at line 3
- **AND** the `mssql` file embeds `SELECT id FROM orders WHERE id = @id` at line 5, `SELECT id FROM refunds` at line 7 and `SELECT id FROM carts WHERE id = ?` at line 9

#### Scenario: A regular expression exec reads nothing
- **WHEN** a file has `/SELECT/.exec('SELECT id FROM nothing')`
- **THEN** the file embeds no document and has no unread site

#### Scenario: A call on a value and a sql tag read nothing without a driver
- **WHEN** a project that depends on no driver has `getDb().query('SELECT id FROM audits')` and ``sql`SELECT id FROM carts` ``
- **THEN** the file embeds no document

### Requirement: Raw SQL of the other SQL drivers
With the `sql-drivers` pack, the first argument of `execute` of `@planetscale/database` and `oracledb`, and of `query` of `sequelize`, SHALL be an embedded `sql` document, bind markers such as `?` and `:id` included.

#### Scenario: A query of another SQL driver is read
- **WHEN** a project that depends on `@planetscale/database` has `conn.execute('SELECT id FROM users WHERE id = ?', [id])`, one on `oracledb` has `connection.execute('SELECT id FROM employees WHERE id = :id', [id])`, and one on `sequelize` has `sequelize.query('SELECT id FROM projects', { type: QueryTypes.SELECT })`
- **THEN** each file embeds its statement at the line of the call

## MODIFIED Requirements

### Requirement: Raw SQL of the SQL drivers
With the `sql-drivers` pack, a literal or template first argument of `query`, `execute`, `prepare` and `exec` on a receiver of up to three segments SHALL be an embedded `sql` document, a computed part as `?`. A first argument with no text SHALL record nothing. The pack SHALL apply to a project on `pg`, `mysql`, `mysql2`, `better-sqlite3`, `sqlite3`, `@neondatabase/serverless`, `mssql`, `@libsql/client`, `@planetscale/database`, `oracledb`, `sequelize`, `postgres` or `@vercel/postgres`.

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
