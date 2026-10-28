# Tasks

## 1. Reading

- [x] 1.1 `gsttt-spec`: specs, deltas with their four sections, task lists, errors with file and line
- [x] 1.2 `gsttt-scenario`: argument checking with compile errors at the arguments

## 2. The gate

- [ ] 2.1 Finding proofs with `syn`, `proptest!` bodies included, fixtures and snapshots left out, test attributes required
- [ ] 2.2 The rules, pending scenarios, the report and the exit code
- [ ] 2.3 `--ready`

## 3. Workflow

- [ ] 3.1 `archive-specs.yml` with the OpenSpec CLI pinned and run through `pnpm dlx`, actions pinned by commit

## 4. Proof

- [ ] 4.1 A test annotated `#[scenario("spec-gate", …)]` for every scenario of the `spec-gate` spec, on fixture trees
- [ ] 4.2 `cargo xtask spec` passes on this repository, and `cargo nextest run -p gsttt-spec -p gsttt-scenario -p xtask`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
