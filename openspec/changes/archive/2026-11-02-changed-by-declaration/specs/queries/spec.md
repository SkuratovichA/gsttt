# Spec Delta

## ADDED Requirements

### Requirement: A changed declaration is walked with its file
An `impact` request that names the changed declarations of a file SHALL walk those declarations and the declarations of the file that use them, and a name that is empty SHALL walk nothing. The header SHALL name the declarations and the file.

#### Scenario: A changed declaration is walked with the declarations of its file that use it
- **WHEN** `helper` of `lib.ts` is asked for, `render` uses `helper`, `a.ts` imports `render` and `b.ts` imports `unrelated`
- **THEN** only `a.ts` is affected and the header is `impact of helper in lib.ts, within 1 hop through code`
