# Spec Delta

## ADDED Requirements

### Requirement: Names on the command line
`gsttt impact` SHALL take `--name <NAME>`, which may be repeated, with paths and never with `--changed`, and SHALL ask the query about those names in every path it is given.

#### Scenario: --name asks about a name
- **WHEN** `b.ts` uses `foo` of `a.ts` in `run`, and `gsttt impact a.ts --name foo` runs
- **THEN** the answer lists `b.ts` with the caller `run`

#### Scenario: --name with --changed exits 2
- **WHEN** `gsttt impact --changed --name foo` runs
- **THEN** stderr says `--name` needs paths and the exit code is 2
