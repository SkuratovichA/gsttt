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
