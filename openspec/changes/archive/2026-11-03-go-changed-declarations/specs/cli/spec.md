# Spec Delta

## MODIFIED Requirements

### Requirement: Changed declarations are walked
`gsttt impact --changed` SHALL walk the declarations of a modified file whose text differs from its old blob, not the file whole, for every language that gives its code (TypeScript, Python and Go), and its header SHALL name them. The `--json` answer SHALL list them in `parts`.

#### Scenario: --changed walks the declarations a patch changes
- **WHEN** one exported function of `src/format.ts` is edited, and two files import it and two import another export
- **THEN** `gsttt impact --changed` lists only the importers of the edited function, and starts with `impact of formatUser in src/format.ts, within 1 hop through code`

## ADDED Requirements

### Requirement: A changed Go declaration reaches its callers
A changed function, method, type or field of a Go file SHALL be named and the walk SHALL reach the declarations that use that name and not those that use another name of the file. A comment added to the file SHALL name no declaration, and a changed `init` SHALL walk the file whole.

#### Scenario: --changed follows a changed Go function to its callers and not to the callers of another
- **WHEN** `job/job.go` declares `Run` and `Stop`, `cmd/main.go` calls `Run` and `halt/halt.go` calls `Stop`, and `Run` is edited
- **THEN** `parts` names `Run` in `job/job.go` and only `cmd/main.go` is affected

#### Scenario: --changed names no declaration for a comment added to a Go file
- **WHEN** a comment is added above `Run` in `job/job.go`
- **THEN** `parts` names no declaration of the file and nothing is affected

#### Scenario: --changed names the field a Go struct gains and no sibling
- **WHEN** a field `longerName` is added to a struct with a field `n`, which gofmt realigns
- **THEN** `parts` names `Box.longerName` and not `Box.n`

#### Scenario: --changed walks a Go file whole when init changes
- **WHEN** the body of an `init` function in `job/job.go` is edited
- **THEN** the callers of every declaration of the file are affected
