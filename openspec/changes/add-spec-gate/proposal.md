# Proposal

## Why

Specs drift from code unless something fails when they do. Tying each scenario to one test by name, and checking that tie with `cargo xtask spec`, makes a scenario a promise the build keeps, and lets work land change by change without breaking the build while a change is half done. This change runs the gate in the archive workflow, on the specs it produces; the CI workflow that runs it on pull requests and pushes comes with `add-distribution`.

## What Changes

- `gsttt-spec`: a reader of OpenSpec specs, deltas and task lists, with lines.
- `gsttt-scenario`: the `#[scenario(capability, scenario)]` attribute, checked at compile time.
- `cargo xtask spec`: the gate, with pending scenarios for unfinished changes, and `--ready` for the archive workflow.
- `.github/workflows/archive-specs.yml`: archives finished changes on `main`.

## Capabilities

### New Capabilities

- `spec-gate`: how specs and tests are tied together and checked.

### Modified Capabilities

None.

## Impact

`crates/spec/`, `crates/scenario/`, `xtask/src/spec.rs` with `xtask/src/spec/`, and the archive workflow. Every other crate uses `gsttt-scenario` as a dev-dependency, and `gsttt-scenario` itself uses `trybuild` for the compile-fail tests of the attribute.

The root `Cargo.toml` pins `serde_json` with `preserve_order` for every crate, so that a run of one package (`-p`) and a run of the workspace (`--workspace`) print JSON keys in the same order; this change carries that pin.
