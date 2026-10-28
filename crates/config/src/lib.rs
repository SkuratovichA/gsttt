//! The Lua side of gsttt: the config in `.gsttt/init.lua`, rule packs, and connectors that run as a command.
//!
//! - [`pack`]: a rule pack is a Lua chunk that returns one table; [`eval_pack`] reads it into a [`Pack`](gsttt_core::rules::Pack).

pub mod pack;
mod sandbox;

pub use pack::{PackError, builtin_packs, eval_pack};
