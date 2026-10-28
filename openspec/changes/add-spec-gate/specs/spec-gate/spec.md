# Spec Delta

## Purpose

The spec gate keeps the specs and the tests honest with each other: every scenario in `openspec/specs/` is proven by exactly one test, every test that claims a scenario names one that exists, and a change cannot be finished while its scenarios are unproven. It runs as `cargo xtask spec`, and the archive workflow folds finished changes into the specs.

## ADDED Requirements

### Requirement: Reading specs
The `gsttt-spec` crate SHALL read OpenSpec files into capabilities, requirements and scenarios with their files and lines: `openspec/specs/<capability>/spec.md`, and for each change under `openspec/changes/` other than `archive`, its `tasks.md` checkboxes and its deltas `specs/<capability>/spec.md` with their `ADDED`, `MODIFIED`, `REMOVED` and `RENAMED` sections. A file that does not follow the format, or that opens a code fence it never closes, SHALL be an error naming the file and line.

#### Scenario: The repository's own specs read cleanly
- **WHEN** the gate reads this repository's `openspec/` directory
- **THEN** it reads every spec and change without an error, and finds every scenario heading with its line

#### Scenario: A malformed scenario is named
- **WHEN** a spec has a `#### Scenario:` heading with no `WHEN` line under it
- **THEN** reading fails with the file and the line of the heading

#### Scenario: An unclosed code fence is named
- **WHEN** a spec or a delta opens a code fence that nothing closes
- **THEN** reading fails with the file and the line that opens the fence, even when the fence hides the steps of the scenario above it

### Requirement: The scenario attribute
`#[scenario("<capability>", "<scenario>")]` SHALL take exactly two string literals and SHALL leave the function it marks unchanged; any other arguments SHALL be a compile error.

#### Scenario: Wrong arguments do not compile
- **WHEN** a function is marked `#[scenario("only-one")]` or `#[scenario(capability, "x")]`
- **THEN** the attribute reports an error at its arguments

### Requirement: Finding proofs
The gate SHALL find `#[scenario]` attributes in the sources that the library, binary and test targets of the workspace compile, following `mod` declarations, `#[path]` attributes and `#[cfg(test)]` modules from each crate root that `cargo metadata` names, including attributes inside `proptest!` blocks. An attribute SHALL count as a proof only on a function that is also marked with an attribute whose last path segment is `test`, or that is inside `proptest!`.

#### Scenario: Proofs inside proptest count
- **WHEN** a test file has `proptest! { #[test] #[scenario("queries", "HTTP matching is symmetric")] fn symmetric(…) { … } }`
- **THEN** the gate counts it as the proof of that scenario

#### Scenario: A marked helper is no proof
- **WHEN** `#[scenario]` marks a function with no test attribute
- **THEN** the gate fails and names the function's file and line

#### Scenario: Modules of a target are followed
- **WHEN** a test is marked `#[scenario]` in a module that a target declares with `mod name;`, with `#[path = "…"]` or with `#[cfg(test)]`, whether the module is a file of its own, a `mod.rs` or inside an inline module
- **THEN** the gate counts it as the proof of that scenario

#### Scenario: A target outside the member is read
- **WHEN** a `[[test]]` target has a `path` outside its package's directory and a test in that file is marked `#[scenario]`
- **THEN** the gate counts it as the proof of that scenario

### Requirement: Proofs that do not run
A `#[scenario]` attribute SHALL prove nothing, and the gate SHALL fail and name its file, line and reason, when its test would not run in `cargo test`: the function is `#[ignore]`d, it or a module around it or its file is compiled only under a `cfg` other than `test`, its target needs `required-features`, or no library, binary or test target reaches its file.

#### Scenario: A file no target reaches is no proof
- **WHEN** a test marked `#[scenario]` is in a `.rs` file that no `mod` declaration of any target reaches
- **THEN** the gate fails and names the test's file and line

#### Scenario: An ignored test is no proof
- **WHEN** a test marked `#[scenario]` is also marked `#[ignore]`
- **THEN** the gate fails and names the test's file and line

#### Scenario: A conditional test is no proof
- **WHEN** a test marked `#[scenario]` is compiled only under `#[cfg(feature = "slow")]`, through its function, a module around it, its file, or the `required-features` of its target
- **THEN** the gate fails and names the test's file and line

### Requirement: Files that are read
The gate SHALL look for `.rs` files that no target reaches by walking each package, leaving out `target`, `fixtures` and `snapshots` directories, hidden directories, directories with a `Cargo.toml` of their own and symbolic links, and an entry it cannot read SHALL be an error naming that entry. The reader of `openspec/` SHALL not follow symbolic links either.

#### Scenario: Copies of the tree are left out
- **WHEN** a package holds copies of the workspace under `target/`, in a hidden directory or in a directory with a `Cargo.toml` of its own, and the copies hold tests marked `#[scenario]`
- **THEN** the gate reads none of them

### Requirement: The gate
`cargo xtask spec` SHALL fail when a scenario in `openspec/specs/` has no proof, when a proof names a scenario that is in neither `openspec/specs/` nor the `ADDED` or `MODIFIED` requirements of an open change, when a scenario has more than one proof, when two scenarios of one capability share a title, or when a proof names a scenario that an open change takes away, by removing its requirement or by modifying it without that scenario.

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

#### Scenario: Retitled scenarios may not be claimed
- **WHEN** an open change modifies a requirement so that it no longer has a scenario of `openspec/specs/`, and a test still claims that scenario
- **THEN** the gate exits non-zero and names the test and the change

### Requirement: Pending scenarios
A scenario that an open change adds or modifies and that has no proof SHALL be reported as pending while that change has an unticked task or no task at all, saying which, and SHALL fail once all its tasks are ticked.

#### Scenario: An open change is pending
- **WHEN** an open change with an unticked task adds a scenario that has no proof
- **THEN** the gate exits zero and reports the scenario as pending

#### Scenario: A change without tasks is pending
- **WHEN** an open change has no `tasks.md`, or none with a checkbox in it, and adds a scenario that has no proof
- **THEN** the gate exits zero and reports the scenario as pending because the change has no tasks

#### Scenario: A finished change must be proven
- **WHEN** every task of an open change is ticked and one of its scenarios has no proof
- **THEN** the gate exits non-zero and names that scenario

### Requirement: The report
The report SHALL list, per capability, the scenarios proven, pending and failing, each failure with the spec's file and line and the proofs' files and lines, and the gate SHALL exit non-zero exactly when something fails.

#### Scenario: A proven spec passes
- **WHEN** every scenario in `openspec/specs/` has one proof and no proof names anything else
- **THEN** the gate exits zero and reports every scenario as proven

### Requirement: Archiving
`cargo xtask spec --ready` SHALL print, one per line, the open changes OpenSpec can archive: every task ticked, every scenario proven, and no failure of the gate naming them. On each push to `main` the `archive-specs` workflow SHALL archive those with the pinned OpenSpec CLI, go on when one fails, commit and push the result if the gate passes on it, and fail at the end if any failed; its push SHALL start no other workflow.

#### Scenario: Ready changes are listed
- **WHEN** one open change has every task ticked and every scenario proven, and another has an unticked task
- **THEN** `--ready` prints the first and not the second

#### Scenario: A change that a failure names is not ready
- **WHEN** a change with every task ticked removes a requirement and a test still claims one of its scenarios
- **THEN** `--ready` does not print the change until the test stops claiming the scenario

### Requirement: Archive order
`--ready` SHALL print a change only when every requirement it modifies, removes or renames is in `openspec/specs/` or added or renamed to by a change printed before it, and no change printed before it removed or renamed away that requirement. It SHALL print a change that removes or renames a requirement after every ready change that modifies it, and the others in the order of their names. Two open changes that remove or rename away the same requirement SHALL fail the gate.

#### Scenario: A change that names a missing requirement is not ready
- **WHEN** a change with every task ticked and every scenario proven modifies, removes or renames a requirement that `openspec/specs/` lacks and no other change adds
- **THEN** `--ready` does not print it, and still prints the other ready changes

#### Scenario: A change comes after the change that adds what it modifies
- **WHEN** a change with every task ticked modifies a requirement that another change adds, and the other change has every task ticked too
- **THEN** `--ready` prints the other change and then the first
- **AND** it prints neither while the other change has a task unticked

#### Scenario: A change that modifies a requirement comes before the change that removes it
- **WHEN** a change with every task ticked and every scenario proven removes a requirement of `openspec/specs/`, another such change modifies it, and the remover has the first name
- **THEN** `--ready` prints the modifier and then the remover

#### Scenario: A change that modifies a requirement comes before the change that renames it
- **WHEN** a change with every task ticked and every scenario proven renames a requirement of `openspec/specs/`, another such change modifies it under its old name, and the renamer has the first name
- **THEN** `--ready` prints the modifier and then the renamer

#### Scenario: A change that modifies a renamed requirement comes after the rename
- **WHEN** a change with every task ticked and every scenario proven renames a requirement of `openspec/specs/`, another such change modifies it under its new name, and the modifier has the first name
- **THEN** `--ready` prints the renamer and then the modifier

#### Scenario: Two changes that take one requirement away fail
- **WHEN** two open changes remove the same requirement of `openspec/specs/`, or one removes it and the other renames it away
- **THEN** the gate exits non-zero and names both changes, the capability and the requirement, with the line of each delta's entry
- **AND** `--ready` prints neither change

### Requirement: Purposes
A spec whose `## Purpose` is empty or is the placeholder that `openspec archive` writes when a delta gives none (`TBD - created by archiving change <name>. Update Purpose after archive.`) SHALL fail the gate, at the line of the heading or of the placeholder. So SHALL an open change, finished or not, whose delta adds requirements to a capability that `openspec/specs/` lacks and has no or an empty `## Purpose`, at the line of its first requirement; `--ready` SHALL NOT print that change.

#### Scenario: A placeholder purpose fails
- **WHEN** a spec in `openspec/specs/` has the purpose `TBD - created by archiving change add-indexing-engine. Update Purpose after archive.`
- **THEN** the gate exits non-zero and names the spec's file and the line of the placeholder

#### Scenario: An empty purpose fails
- **WHEN** a spec in `openspec/specs/` has a `## Purpose` heading with no prose under it
- **THEN** the gate exits non-zero and names the spec's file and the line of the heading
- **AND** it fails a finished and proven change whose delta creates a capability under an empty `## Purpose`, saying that the purpose is empty, and `--ready` does not print the change

#### Scenario: A change that creates a capability needs a purpose
- **WHEN** a finished and proven change adds requirements to a capability that `openspec/specs/` lacks and its delta has no `## Purpose`
- **THEN** the gate exits non-zero and names the delta's file and the line of its first requirement, and `--ready` does not print the change until the delta has a purpose

#### Scenario: An unfinished change needs a purpose too
- **WHEN** an open change with an unticked task adds requirements to a capability that `openspec/specs/` lacks and its delta has no `## Purpose`
- **THEN** the gate exits non-zero and names the delta's file and the line of its first requirement

### Requirement: Changes on top of each other
The gate SHALL read the open changes in the order `--ready` would archive them, applying each to what the main specs and the changes before it leave: a requirement it adds is put in, one it modifies replaces the one with that name, one it removes is taken away, one it renames has its new name. The scenarios of what is left need a proof, and no test may claim one that a change took away. Changes that cannot be placed come last, in the order of their names, and apply as far as their deltas reach.

#### Scenario: A title that a later change keeps counts once
- **WHEN** an open change modifies a requirement that another open change adds, and keeps one of its scenario titles
- **THEN** the title counts once, as the later change's, and one proof of it passes

#### Scenario: A requirement that a later change removes needs no proof
- **WHEN** an open change removes a requirement that another open change adds, and no test claims its scenarios
- **THEN** the gate exits zero

#### Scenario: Scenarios that a later change removes may not be claimed
- **WHEN** an open change removes a requirement that another open change adds, and a test still claims one of its scenarios
- **THEN** the gate exits non-zero and names the test and the change that removes the requirement

#### Scenario: A renamed requirement is found under its new name
- **WHEN** an open change renames a requirement that another open change adds, and a third open change modifies it under the new name and keeps one of its scenario titles
- **THEN** the title counts once, as the third change's

#### Scenario: A change that nothing places still counts
- **WHEN** an open change modifies a requirement that no spec and no other open change has
- **THEN** its scenarios need a proof like those of any other change, and `--ready` does not print the change
