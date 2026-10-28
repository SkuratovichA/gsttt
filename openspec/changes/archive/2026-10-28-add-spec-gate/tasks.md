# Tasks

## 1. Reading

- [x] 1.1 `gsttt-spec`: specs, deltas with their four sections, task lists, errors with file and line
- [x] 1.2 `gsttt-scenario`: argument checking with compile errors at the arguments

## 2. The gate

- [x] 2.1 Finding proofs with `syn`, `proptest!` bodies included, fixtures and snapshots left out, test attributes required
- [x] 2.2 The rules, pending scenarios, the report and the exit code
- [x] 2.3 `--ready`

## 3. Workflow

- [x] 3.1 `archive-specs.yml` with the OpenSpec CLI pinned and run through `pnpm dlx`, actions pinned by commit

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("spec-gate", …)]` for every scenario of the `spec-gate` spec, on fixture trees
- [x] 4.2 `cargo xtask spec` passes on this repository, and `cargo nextest run -p gsttt-spec -p gsttt-scenario -p xtask`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 Count a proof only when a library, binary or test target reaches it, and name the `#[scenario]` attributes that `#[ignore]`, a `cfg`, `required-features` or an unreached file keep from running
- [x] 5.2 Leave `target`, hidden directories, directories with a `Cargo.toml` of their own and symbolic links out of the walk over a package, and name the failing entry in an I/O error
- [x] 5.3 Report a `proptest!` attribute that does not parse instead of dropping it
- [x] 5.4 Test the workspace listing and the mapping from the verdict to the exit code
- [x] 5.5 Test the compile errors of `#[scenario]` through the attribute itself with `trybuild`
- [x] 5.6 Keep a change out of `--ready` when it modifies, removes or renames a requirement that `openspec/specs/` does not have
- [x] 5.7 Archive the other changes when OpenSpec refuses one and fail the job at the end, rebase before pushing, set a timeout, run cargo with `--locked`, and drop the redundant actor condition
- [x] 5.8 Report a change with no tasks as having none, not as having unticked tasks
- [x] 5.9 Pin the filter that holds back a change named in a failure with a test of a change that removes a requirement a test still claims
- [x] 5.10 Refuse a spec or delta with a code fence that is never closed, naming the line that opens it
- [x] 5.11 Keep the reader of `openspec/` from following symbolic links, which read the same specs again under longer capability names
- [x] 5.12 Drop the unused `pulldown-cmark` dependency and make the proposal and the purpose say what the change does: where the gate runs, the files it touches, the `serde_json` pin
- [x] 5.13 Say in the spec that a proof of a scenario fails when an open change takes the scenario away by modifying its requirement, and pin the failure with that scenario

## 6. Integration fixes

- [x] 6.1 Compile the tests of nine scenarios of earlier changes on every target by taking `#[cfg(unix)]` off them and off the helpers they share, because the gate counts no proof under a `cfg` other than `test` and gsttt builds for unix only
- [x] 6.2 Write the third-level heading that is not a requirement as `### Rule: R` in the reader's test, so that `typos` no longer flags the misspelt word that stood in it
- [x] 6.3 Read the `## Purpose` of specs and deltas, and fail a spec whose purpose is the placeholder that `openspec archive` writes and an open change whose delta creates a capability without a `## Purpose`, so that no archived spec lands without the purpose its change gave it
- [x] 6.4 List the ready changes in an order that `openspec archive` can take them in, so that a change that modifies, removes or renames a requirement comes after the change that adds it, which the archive would otherwise not find
- [x] 6.5 List a change that removes or renames a requirement after the ready changes that modify it, and fail the gate when two open changes remove or rename away the same requirement, so that `openspec archive` does not stop at a requirement an earlier change took away
- [x] 6.6 Tell an empty `## Purpose` from a missing one in the reader, and fail a spec with an empty purpose and an open change, finished or not, whose delta creates a capability with an empty or no purpose, so that no empty section reaches `openspec archive`, which would write the placeholder for it
- [x] 6.7 Test that a `## Purpose` heading with trailing spaces and a spec with Windows line endings give the same purpose and line as without them, which the reader already handled and nothing pinned
- [x] 6.8 Name the key of a requirement or a scenario once in the rules and collect the requirements of the main specs in the walk that finds their scenarios, so that the rules neither spell the tuple out in four places nor walk the main specs twice
- [x] 6.9 Fold the open changes over the main specs in the order `--ready` would archive them, so that a requirement that a change modifies, renames or removes after another open change adds it counts as the later change leaves it, and a test that still claims a scenario the later change took away fails and names that change
- [x] 6.10 Fail an open change whose modified requirement leaves out a scenario of the requirement it replaces in the fold, counting a title as often as it is written, so that the gate says before `openspec archive` does that the archive refuses the block, and take the retitle scenario out of the spec, which described a block that the archive refuses
- [x] 6.11 Leave a modified requirement that `openspec archive` refuses out of the fold, so that the requirement it would replace stands with its scenarios, whose proofs `--ready` asks of the change that wrote them, and a title that only the refused block holds is claimed for nothing
- [x] 6.12 Read a finished change before an unfinished one where the order leaves a choice, so that an open change with the first name does not make the gate refuse a finished change that leaves out a scenario the open one adds, which the archive does not refuse because it takes the finished change first
- [x] 6.13 Test which block a modified requirement is compared with when two open changes add its name, finished or not, that a refusal names a renamed requirement by its new name and fails an unfinished change, and that it names two missing scenarios in the plural, which the gate already did and nothing pinned
- [x] 6.14 Name a title once in the failure of a modified requirement that leaves it out, however often the requirement it replaces holds it, so that the failure does not list one scenario as two
- [x] 6.15 Fail an open change that renames a requirement to a name that already stands, in the main specs or in an earlier change, and leave the rename out of the fold, so that the gate says before `openspec archive` does that the archive refuses the rename, and `--ready` holds the change back
- [x] 6.16 Name the requirement in the failure for a claim on a scenario that a change took away by the name it stood under when the change took it, so that a change that removes a renamed requirement is not said to remove a requirement that its delta never names
- [x] 6.17 Write the requirement of a delta as one that stands before the change is archived, in the main specs or in an earlier open change, in the docs of `main_name`, `Removed::name`, `Renamed::from` and `Delta::modified`, and name in the doc of the fold the places that hold a name more than once, each doc on one line
- [x] 6.18 Keep the reason of a claim on a scenario that a modified requirement takes away and pin the one path left to it, the scenarios of the second block of a name that two changes add, with a test, now that a block that leaves a scenario out is refused and no longer reaches it
