//! The Lua side of gsttt: the config in `.gsttt/init.lua`, rule packs, and connectors that run as a command.
//!
//! - [`evaluate()`] runs the config in a sandbox and returns the [`Setup`](gsttt_core::Setup); [`fingerprint()`] hashes it.
//! - [`pack`]: a rule pack is a Lua chunk that returns one table; [`eval_pack`] reads it into a [`Pack`](gsttt_core::rules::Pack).

mod error;
mod evaluate;
mod fingerprint;
pub mod pack;
mod sandbox;
mod value;

pub use error::ConfigError;
pub use evaluate::evaluate;
pub use fingerprint::fingerprint;
pub use pack::{PackError, builtin_packs, eval_pack};
