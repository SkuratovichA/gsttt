# Spec Delta

## ADDED Requirements

### Requirement: Impact prints the lines it can check
`gsttt impact` SHALL read the files it shows a line for from the working tree and print `path:line` with the line under it as `> text` for a file at distance 1 whose line the file still holds, and with `--json` the entry SHALL carry `at` and `snippet`. With `--no-refresh` over an index the tree has moved away from, it SHALL print no line.

#### Scenario: Impact prints the line of a call
- **WHEN** `src/b.ts` calls `foo` of `src/a.ts` on line 3, and `gsttt impact src/a.ts --name foo` is run
- **THEN** the output has `src/b.ts:3  distance 1  imports foo from src/a.ts; in run (line 2)` and under it `> return foo();`
- **AND** with `--json` the entry has `at` 3 and `snippet` `return foo();`

#### Scenario: A stale index prints no line
- **WHEN** the index is stored, `src/b.ts` is edited, and `gsttt --no-refresh impact src/a.ts --name foo` is run
- **THEN** the output lists `src/b.ts` and prints no line and no text for it

### Requirement: Impact counts generated files apart
`gsttt impact` SHALL count the files tagged as generated apart from the affected files, as `generated: N files (ask with --generated)`, and SHALL list them in a group of their own with `--generated`.

#### Scenario: Generated files are counted unless asked for
- **WHEN** `src/a.ts` is imported by `src/b.ts` and by `src/api.generated.ts`
- **THEN** `gsttt impact src/a.ts` lists `src/b.ts` and says `generated: 1 file (ask with --generated)`
- **AND** `gsttt impact src/a.ts --generated` lists `src/api.generated.ts` under `generated files (1):`
