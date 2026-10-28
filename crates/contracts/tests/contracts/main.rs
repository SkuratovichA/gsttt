//! Tests of the contract-format connectors, run through the engine's pipeline on the fixture trees under
//! `tests/fixtures`. Every scenario of the `contract-formats` capability has its test here, named by
//! `#[gsttt_scenario::scenario]`.

#[cfg(test)]
mod claims;
#[cfg(test)]
mod env;
#[cfg(test)]
mod graphql;
#[cfg(test)]
mod openapi;
#[cfg(test)]
mod prisma;
#[cfg(test)]
mod proto;
#[cfg(test)]
mod provides;
#[cfg(test)]
mod sql;
#[cfg(test)]
mod support;
