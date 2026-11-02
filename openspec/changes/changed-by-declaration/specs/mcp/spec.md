# Spec Delta

## ADDED Requirements

### Requirement: Changed asks by declaration
`impact` with `changed` set to true SHALL answer as the query does for the declarations the patch changes, as the command line does.

#### Scenario: changed asks about the declarations a patch changes
- **WHEN** `lib/util.ts` is modified in `render` and `src/a.ts` imports `render` and `src/b.ts` imports `unrelated`
- **THEN** the text is the one of the query for that `render`, and lists `src/a.ts` and not `src/b.ts`
