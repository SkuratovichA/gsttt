//! Scenario tests of the Rust connector, run through the engine on the fixture trees under `tests/fixtures/`, and one
//! test that runs it over this repository.

#[cfg(test)]
mod support;

mod activation;
mod data;
// Its helpers read the engine's JSON with `unwrap`, which clippy lets pass only in test code.
#[cfg(test)]
mod dogfood;
mod editions;
mod env;
mod gates;
mod imports;
mod modules;
mod packs;
mod provides;
mod requests;
mod routes;
mod scopes;
mod workspace;
