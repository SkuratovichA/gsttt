# Spec Delta

## MODIFIED Requirements

### Requirement: The impact command

`gsttt impact` SHALL take either paths or `--changed`, never both. A named path that is not on disk SHALL be passed as deleted. `--changed` SHALL take the files that differ from `HEAD`, or from the best common ancestor of `HEAD` and `<rev>` with `--base <rev>`, as `Engine::changed_files` lists them: modified, added and untracked files as changed, and deleted files as deleted. `--depth N` SHALL limit the hops through code the answer follows, and `--no-tests` SHALL leave the tests out of it.

#### Scenario: --changed includes untracked files
- **WHEN** a new file is created and not added to git, and `gsttt impact --changed` runs
- **THEN** the new file is among the changed files of the answer

#### Scenario: --changed passes a deleted file as deleted
- **WHEN** `lib/util.ts` is deleted after it was committed, and `gsttt impact --changed` runs
- **THEN** the answer lists `lib/util.ts` as deleted

#### Scenario: A named path that is not on disk counts as deleted
- **WHEN** `lib/util.ts` is deleted, and `gsttt impact lib/util.ts` runs
- **THEN** the answer lists `lib/util.ts` as deleted

#### Scenario: --base without --changed exits 2
- **WHEN** `gsttt impact --base main lib/util.ts` runs
- **THEN** stderr says `--base` needs `--changed` and the exit code is 2

#### Scenario: --changed outside git exits 2
- **WHEN** `gsttt impact --changed` runs in a directory outside any git repository
- **THEN** stderr says `--changed` needs a git repository and the exit code is 2

#### Scenario: --no-tests leaves the tests out
- **WHEN** `a.ts` is imported by `b.ts` and by the test files `a.test.ts` and `b.test.ts`, and `gsttt impact a.ts --no-tests` runs
- **THEN** the answer lists `b.ts`, lists no tests, and says it left out 2 test files
- **AND** `gsttt --json impact a.ts --no-tests` has an empty `tests` and a `tests_left_out` of 2
