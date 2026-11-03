## ADDED Requirements

### Requirement: Heads found across files
A head that the link of a connector finds for a name, and no file writes, SHALL be followed as a written head is: the callers of the same member of the head are listed with a note, and the head lists the names that take it.

#### Scenario: A head found across files is followed
- **WHEN** the link gives `PG` of `pg.go` the head `Store` of `store.go`, and `use.go` calls `Store.Get`
- **THEN** impact for `PG.Get` lists `use.go`, and impact for `Store.Get` lists `pg.go` as overriding it
