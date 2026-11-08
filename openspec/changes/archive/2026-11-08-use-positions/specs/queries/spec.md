# Spec Delta

## ADDED Requirements

### Requirement: A nearest file shows where it refers to the change
`impact` SHALL give a file at distance 1 the line where it uses or imports what the walk came from, read from the file as it is on disk: `path:line` in the text with the line under it as `> text`, and `at` and `snippet` in the JSON. It SHALL do so only for the first 30 files and the first 30 tests, only when the index is fresh, and only when that line holds the last identifier of the name used, or for an import its specifier or a name it takes.

#### Scenario: A caller at distance 1 shows the line of its call and its text
- **WHEN** `b.ts` imports `foo` from `a.ts`, `run` in `b.ts` starts at line 2 and uses `foo` on line 3, which reads `  return foo();`, and impact is asked about `foo` in `a.ts` with the files as they are on disk
- **THEN** the text lists `b.ts:3  distance 1  imports foo from a.ts; in run (line 2)` followed by the row `> return foo();`
- **AND** the JSON entry of `b.ts` has `at` 3 and `snippet` `return foo();`

#### Scenario: An import with no use of its own shows the line of the import
- **WHEN** `b.ts` imports `foo` from `a.ts` on line 1, which reads `import { foo } from './a';`, no declaration of `b.ts` uses it, and impact is asked about `foo` in `a.ts`
- **THEN** the text lists `b.ts:1  distance 1  imports foo from a.ts` followed by the row `> import { foo } from './a';`

#### Scenario: A line the file has moved away from is not shown
- **WHEN** the index records the use of `foo` in `b.ts` on line 3 and line 3 of the file on disk no longer holds `foo`
- **THEN** `b.ts` is still listed at distance 1 with its callers, and the text and the JSON show no line and no text for it

#### Scenario: Only a fresh answer shows lines, and only nearest files within the first thirty
- **WHEN** 32 files import `a.ts` and use `foo`, `c.ts` is reached at distance 2, and impact is asked about `foo` once from a fresh index and once from a stale one
- **THEN** the fresh answer reads 30 files and shows a line for 30, and none for `c.ts`
- **AND** the stale answer reads no file and shows no line

#### Scenario: A long line is cut around the name
- **WHEN** the line of the use holds 500 characters and `foo` starts at the 300th
- **THEN** the text of the line is at most 160 characters, holds `foo` and shows `…` where it was cut

### Requirement: Generated files are counted apart
`impact` SHALL leave the files tagged as generated out of the affected files, and out of the tests unless they are tests, and SHALL count them, as `generated: N files` with the hint of the request in brackets. It SHALL list them in a group of their own only when asked. The walk is the same either way, and a file that is a test and generated stays with the tests.

#### Scenario: Generated files are counted and not listed
- **WHEN** `b.ts` and the generated `gen.ts` import `a.ts`, and impact is asked to leave the generated files out, with the hint `ask with generated=true`
- **THEN** impact lists `b.ts` and says `generated: 1 file (ask with generated=true)`
- **AND** the JSON holds `generated_left_out` 1 and no hint

#### Scenario: Generated files are listed when asked
- **WHEN** `b.ts` and the generated `gen.ts` import `a.ts` and impact is not asked to leave the generated files out
- **THEN** impact lists `b.ts` under affected files and `gen.ts` under generated files, and says nothing of a count

#### Scenario: A generated test stays with the tests
- **WHEN** the test file `a.test.ts`, also tagged as generated, imports `a.ts`
- **THEN** impact lists it under tests and counts no generated file
