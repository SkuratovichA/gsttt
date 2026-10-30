# Spec Delta

## ADDED Requirements

### Requirement: A repository the speed benchmark leaves out
A repository pinned with `speed = false` SHALL NOT be measured by `cargo xtask bench`, and naming it with `--repository` SHALL be an error that says so. Such a repository SHALL have an `agents` table, since nothing else uses it, and the agent benchmarks SHALL still draw questions about it.

#### Scenario: A repository that only the agent benchmarks ask about is left out of the speed benchmark
- **WHEN** the speed benchmark selects the repositories of the committed pins, and one of them is named that has `speed = false`
- **THEN** the repositories selected are those with `speed` true, the one named is an error, and the agent benchmarks still ask about it
