//! The Lua side of gsttt: the config in `.gsttt/init.lua`, rule packs, and connectors that run as a command.
//!
//! - [`evaluate()`] runs the config in a sandbox and returns the [`Setup`](gsttt_core::Setup); [`fingerprint()`] hashes it.
//! - [`pack`]: a rule pack is a Lua chunk that returns one table; [`eval_pack`] reads it into a [`Pack`](gsttt_core::rules::Pack).
//! - [`annotations()`], [`luarc()`] and [`starter()`] are what `gsttt init` writes so that an editor completes and checks the config.
//! - [`external`]: an [`External`] is the connector for one `g.external { … }` of the setup, and [`protocol_schema()`] is the JSON schema of the messages it exchanges with its process.

mod editor;
mod error;
mod evaluate;
pub mod external;
mod fingerprint;
pub mod pack;
mod sandbox;
#[cfg(test)]
mod testlog;
mod text;
mod value;

pub use editor::{ANNOTATIONS_PATH, LUARC_PATH, annotations, luarc, starter};
pub use error::ConfigError;
pub use evaluate::{INIT_PATH, evaluate};
pub use external::protocol::{PROTOCOL, schema as protocol_schema};
pub use external::{External, ExternalProject};
pub use fingerprint::fingerprint;
pub use pack::{PackError, builtin_packs, eval_pack};
