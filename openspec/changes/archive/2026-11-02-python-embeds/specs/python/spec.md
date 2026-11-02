## ADDED Requirements

### Requirement: Packs that find SQL in the provides
The provides of a project SHALL name every pack whose `[[embeds]]` rules find SQL, as well as the packs whose other rules name a table, in `via` of the `table` calls.

#### Scenario: The dbapi pack is credited with the tables it finds
- **WHEN** a project with no manifest has a file that calls `cursor.execute("SELECT id FROM bookings")`
- **THEN** the provides name `dbapi` in `via` of the `table` calls

#### Scenario: The sqlalchemy pack is credited with the tables it finds
- **WHEN** a project that depends on `sqlalchemy` has a file that calls `connection.execute(text("SELECT id FROM bookings"))`
- **THEN** the provides name `dbapi` and `sqlalchemy` in `via` of the `table` calls
