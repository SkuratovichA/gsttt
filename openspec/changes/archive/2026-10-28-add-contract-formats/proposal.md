# Proposal

## Why

Code that serves a route and code that calls it rarely import each other; what ties them together is a schema, a spec or a migration. Reading those files as declarations gives every contract a hub, so a change to `openapi.yaml`, `schema.graphql` or a migration reaches the code on both sides, and a change on one side reaches the other through the declaration.

## What Changes

- The crate `gsttt-contracts` with six connectors: `openapi` (with `$ref` imports and fragments), `graphql` (schemas, operations, embedded documents), `proto` (services, HTTP bindings, imports through buf roots), `prisma`, `sql` (DDL declares, DML calls, embedded documents, driver placeholders) and `env`.
- `embeds()` and `parse_embedded` for `graphql` and `sql`, so documents found inside TypeScript, Python and Rust are read here.

## Capabilities

### New Capabilities

- `contract-formats`: which files each contract connector claims and which sites it reads from them.

### Modified Capabilities

None.

## Impact

`crates/contracts/` only, plus new entries in the root `[workspace.dependencies]` if needed. Known gaps, reported as blind spots: GraphQL fragments spread from other files, environment variables set in compose files, CI and deployment manifests, and schemas generated at build time.
