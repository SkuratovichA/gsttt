# Proposal

## Why

The index is only worth building for the questions it answers. The main one is impact: what a change can break, across imports and across the contracts between services, which is what an agent needs before it edits shared code and what a reviewer needs before approving it. The answers have to be short enough for an agent's context and honest about what they did not see.

## What Changes

- `impact` over reverse import edges and contracts, with distances, reasons, a test group, deleted and unknown inputs.
- `dependents`, `dependencies`, `path`, `contract`, `coverage` and `status`.
- HTTP key matching with parameters, wildcards and `http_prefixes`.
- Answer types that render as text (`Display`) and JSON (`Serialize`): relative paths, `path:line` sites, lists cut at 30 with `… N more`, an account of what was searched when nothing is found, and a coverage and freshness footer.

## Capabilities

### New Capabilities

- `queries`: questions about an index and the shape of their answers.

### Modified Capabilities

None.

## Impact

`crates/core/src/query.rs` and `crates/core/src/query/` (new), query tests in `crates/core/tests/`. The queries read `Index`, `Graph`, `Coverage` and `Freshness` as they are and add no dependencies.
