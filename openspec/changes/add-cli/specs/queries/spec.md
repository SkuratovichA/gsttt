# Spec Delta

## Purpose

Queries read an index and answer what a change affects and how files and contracts connect, as short text for people and agents and as JSON for scripts. Every answer says what it searched, how much of the tree was read and how fresh the index is.

## MODIFIED Requirements

### Requirement: Coverage answer
`coverage` SHALL begin with the problems of the index, one line each (the key `problems` in JSON), and SHALL list, per connector, its projects, its files by parse status (read cleanly, read past syntax errors, failed, skipped), the share of imports resolved, the unresolved imports and unread sites by reason with the first place of each, and what it provides with its blind spots, followed by the unclaimed files by language and the files skipped before any connector read them.

#### Scenario: Coverage names failures and blind spots
- **WHEN** an index has a TypeScript file that failed to parse, an unresolved import and a `fetch` with a computed URL
- **THEN** the coverage answer names the failed file with its failure, the import's reason with its first place and the unread site's reason with its first place

#### Scenario: A skipped file is owned and not read
- **WHEN** `ts` owns four files, of which one is over the size limit and one is not UTF-8, so that the engine lists both as skipped
- **THEN** the coverage answer gives for `ts` 2 files read cleanly, 0 read past syntax errors, 0 failed and 2 skipped
- **AND** its coverage line says `ts read 2 of 4 files`

#### Scenario: Coverage lists the problems of the index
- **WHEN** the index holds two problems, a connector call that failed and an external connector that waits for trust
- **THEN** the coverage text begins with `problems (2):` and one indented line for each, sorted, before the first connector
- **AND** its JSON holds both under `problems`, which is an empty list when the index holds none
