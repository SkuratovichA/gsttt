# Spec Delta

## Purpose

The spec gate keeps the specs and the tests honest with each other: every scenario in `openspec/specs/` is proven by exactly one test, every test that claims a scenario names one that exists, and a change cannot be finished while its scenarios are unproven. It runs as `cargo xtask spec` in CI, and the archive workflow folds finished changes into the specs.

## ADDED Requirements

### Requirement: Reading specs
The `gsttt-spec` crate SHALL read OpenSpec files into capabilities, requirements and scenarios with their files and lines: `openspec/specs/<capability>/spec.md`, and for each change under `openspec/changes/` other than `archive`, its `tasks.md` checkboxes and its deltas `specs/<capability>/spec.md` with their `ADDED`, `MODIFIED`, `REMOVED` and `RENAMED` sections. A file that does not follow the format SHALL be an error naming the file and line.

#### Scenario: The repository's own specs read cleanly
- **WHEN** the gate reads this repository's `openspec/` directory
- **THEN** it reads every spec and change without an error, and finds every scenario heading with its line

#### Scenario: A malformed scenario is named
- **WHEN** a spec has a `#### Scenario:` heading with no `WHEN` line under it
- **THEN** reading fails with the file and the line of the heading

### Requirement: The scenario attribute
`#[scenario("<capability>", "<scenario>")]` SHALL take exactly two string literals and SHALL leave the function it marks unchanged; any other arguments SHALL be a compile error.

#### Scenario: Wrong arguments do not compile
- **WHEN** a function is marked `#[scenario("only-one")]` or `#[scenario(capability, "x")]`
- **THEN** the attribute reports an error at its arguments

### Requirement: Finding proofs
The gate SHALL find every `#[scenario]` attribute in the `.rs` files of the workspace's members, including those inside `proptest!` blocks, leaving out files under directories named `fixtures` or `snapshots`. An attribute SHALL count as a proof only on a function that is also marked with an attribute whose last path segment is `test`, or that is inside `proptest!`.

#### Scenario: Proofs inside proptest count
- **WHEN** a test file has `proptest! { #[test] #[scenario("queries", "HTTP matching is symmetric")] fn symmetric(…) { … } }`
- **THEN** the gate counts it as the proof of that scenario

#### Scenario: A marked helper is no proof
- **WHEN** `#[scenario]` marks a function with no test attribute
- **THEN** the gate fails and names the function's file and line

### Requirement: The gate
`cargo xtask spec` SHALL fail when a scenario in `openspec/specs/` has no proof, when a proof names a scenario that is in neither `openspec/specs/` nor the `ADDED` or `MODIFIED` requirements of an open change, when a scenario has more than one proof, when two scenarios of one capability share a title, or when a proof names a scenario of a requirement an open change removes.

#### Scenario: A scenario without a proof fails
- **WHEN** a scenario in `openspec/specs/queries/spec.md` has no proof
- **THEN** the gate exits non-zero and names the capability, the scenario and its line

#### Scenario: A proof of nothing fails
- **WHEN** a test is marked `#[scenario("queries", "Impact goes through wormholes")]` and no spec has that scenario
- **THEN** the gate exits non-zero and names the test's file and line

#### Scenario: A scenario proven twice fails
- **WHEN** two tests claim the same scenario
- **THEN** the gate exits non-zero and names both tests

#### Scenario: Two scenarios with one title fail
- **WHEN** two requirements of one capability each have a scenario titled `Defaults apply`
- **THEN** the gate exits non-zero and names both lines

#### Scenario: Removed scenarios may not be claimed
- **WHEN** an open change removes a requirement and a test still claims one of its scenarios
- **THEN** the gate exits non-zero and names the test

### Requirement: Pending scenarios
A scenario that an open change adds or modifies and that has no proof SHALL be reported as pending while that change has an unticked task, and SHALL fail once all its tasks are ticked.

#### Scenario: An open change is pending
- **WHEN** an open change with an unticked task adds a scenario that has no proof
- **THEN** the gate exits zero and reports the scenario as pending

#### Scenario: A finished change must be proven
- **WHEN** every task of an open change is ticked and one of its scenarios has no proof
- **THEN** the gate exits non-zero and names that scenario

### Requirement: The report
The report SHALL list, per capability, the scenarios proven, pending and failing, each failure with the spec's file and line and the proofs' files and lines, and the gate SHALL exit non-zero exactly when something fails.

#### Scenario: A proven spec passes
- **WHEN** every scenario in `openspec/specs/` has one proof and no proof names anything else
- **THEN** the gate exits zero and reports every scenario as proven

### Requirement: Archiving
`cargo xtask spec --ready` SHALL print, one per line, the open changes whose tasks are all ticked and whose scenarios are all proven. The `archive-specs` workflow SHALL run on every push to `main`, archive each change it prints with the pinned OpenSpec CLI, run the gate on the result, and commit and push the archived changes and updated specs when the gate passes; a push it makes SHALL start no other workflow.

#### Scenario: Ready changes are listed
- **WHEN** one open change has every task ticked and every scenario proven, and another has an unticked task
- **THEN** `--ready` prints the first and not the second
