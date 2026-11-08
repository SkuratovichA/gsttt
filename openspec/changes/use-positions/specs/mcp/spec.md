# Spec Delta

## ADDED Requirements

### Requirement: Generated files over MCP
`impact` SHALL take `generated`, a boolean that is false unless the call gives it. With `generated` false the answer SHALL count the files tagged as generated and say `ask with generated=true`; with it true the answer SHALL list them. The schema of `impact` SHALL describe `generated` and its default.

#### Scenario: impact counts the generated files unless asked
- **WHEN** `a.ts` is imported by `b.ts` and by the generated `api.generated.ts`, and `impact` is called with `paths` set to `["a.ts"]`
- **THEN** the answer lists `b.ts` and says `generated: 1 file (ask with generated=true)`
- **AND** with `generated` set to true it lists `api.generated.ts` under `generated files (1):`

### Requirement: Impact shows the lines of the files it can read
`impact` SHALL ask the source of the index for a reader of the files as they are on disk and show the lines of the nearest files as the query does. A source with no reader SHALL give the answer without them.

#### Scenario: A source with a reader shows the line, and one without shows none
- **WHEN** `b.ts` calls `foo` of `a.ts` on line 3 and `impact` is called about `foo` in `a.ts` on a source that reads `b.ts`, and on one that reads no file
- **THEN** the first answer lists `b.ts:3  distance 1` and under it `> return foo();`
- **AND** the second lists `b.ts  distance 1` with no line under it
