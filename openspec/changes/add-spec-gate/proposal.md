# Proposal

## Why

Specs drift from code unless something fails when they do. Tying each scenario to one test by name, and checking that tie on every push, makes a scenario a promise the build keeps, and lets work land change by change without breaking the build while a change is half done.

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

`crates/spec/`, `crates/scenario/`, `xtask/src/spec.rs` with `xtask/src/spec/`, `xtask/tests/spec*`, and the archive workflow. Every other crate uses `gsttt-scenario` as a dev-dependency.
