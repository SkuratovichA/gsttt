# Spec Delta

## REMOVED Requirements

### Requirement: DB-API arguments that are not literals
**Reason**: An f-string, a `+` and a `.format` template are no longer unread sites, so two of its scenarios name what is no longer true, and a scenario of a MODIFIED requirement cannot be renamed.
**Migration**: "DB-API arguments built at run time" says what is true now; the scenario "An argument that is not a string records nothing" moves there unchanged.

## ADDED Requirements

### Requirement: DB-API arguments built at run time
With the `dbapi` pack, an `execute` or `executemany` call whose first argument is an f-string, a `+` or a `.format` template SHALL embed an `sql` document with `?` for each computed part, as `text()` does under `sqlalchemy`. A string with `%` on it SHALL be an unread site. A call whose first argument is a variable, an attribute, a subscript or a call SHALL record nothing, and the provides SHALL name SQL built by a function call and a statement with a computed table as blind spots.

#### Scenario: A computed part of a SQL string is read as a placeholder
- **WHEN** a file has `cursor.execute(f"SELECT id FROM bookings WHERE id = {task}")`, the same statement joined with `+` and the same statement as a `.format` template
- **THEN** each call embeds an `sql` document whose text has `?` for the computed part and is no unread site
- **AND** the table `bookings` is called

#### Scenario: A statement whose table is computed names no table
- **WHEN** a file has `cursor.execute(f"SELECT id FROM {table}")`
- **THEN** the call embeds an `sql` document and is no unread site, and no table is called
- **AND** the provides of the project name a statement whose table or whole clause is computed as a blind spot of its tables

#### Scenario: A computed part of text() is read as a placeholder
- **WHEN** a project that depends on `sqlalchemy` has `text(f"SELECT id FROM bookings WHERE id = {task}")`
- **THEN** the call embeds an `sql` document whose text has `?` for the computed part and is no unread site
- **AND** the table `bookings` is called

#### Scenario: A triple-quoted f-string keeps the line of its statement
- **WHEN** a file has an f-string in triple quotes over several lines with a computed part on its last line
- **THEN** the document starts on the line of its first literal and each statement of it keeps its line

#### Scenario: A SQL string formatted with % is an unread site
- **WHEN** a file has `cursor.execute("SELECT id FROM bookings WHERE id = %s" % task)` and one call with the plain literal `"SELECT id FROM bookings"`
- **THEN** the call with `%` is an unread site and embeds no document
- **AND** the call with the plain literal embeds an `sql` document

#### Scenario: A format template with a replacement field is read as a placeholder
- **WHEN** a file has `cursor.execute("SELECT id FROM {}".format(table))`, `cursor.execute("SELECT id FROM {name} WHERE id = {0}".format(task, name=table))`, `cursor.execute("SELECT id FROM {:{width}}".format(table, width=3))` and `cursor.execute("SELECT '{{}}'".format())`
- **THEN** none of them is an unread site
- **AND** the first three embed `sql` documents with `?` for each replacement field, and the last embeds the text `SELECT '{}'`

#### Scenario: An argument that is not a string records nothing
- **WHEN** a file has `pool.execute(task)`, `cursor.execute(queries[0])`, `cursor.executemany(queries.insert, rows)`, `cursor.execute(compose(task))` and `cursor.execute()`
- **THEN** none of them is an unread site and none embeds a document
- **AND** the provides of the project name SQL built by a function call as a blind spot
