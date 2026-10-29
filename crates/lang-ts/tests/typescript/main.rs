//! Scenario tests of the TypeScript connector. Each one runs the whole pipeline on a small tree under `tests/fixtures/` and snapshots the output, so what the connector reads is reviewed like code.

mod data;
mod exports;
mod graphql;
mod http;
mod imports;
mod packs;
mod projects;
mod resolution;
mod support;
mod symbols;
mod trpc;
