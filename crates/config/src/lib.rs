//! The TOML side of gsttt: the config in `.gsttt/config.toml`, rule packs in `.gsttt/packs/`, and connectors that run as a command.
//!
//! - [`evaluate()`] reads the config and the packs into the [`Setup`](gsttt_core::Setup); [`fingerprint()`] hashes it.
//! - [`pack`]: a rule pack is a TOML file with the fields of a [`Pack`](gsttt_core::rules::Pack); [`read_pack()`] reads one.
//! - [`starter()`] is the config `gsttt init` writes, and [`CONFIG_SCHEMA_PATH`] and [`PACK_SCHEMA_PATH`] are where it writes the schemas an editor checks the files with.
//! - [`external`]: an [`External`] is the connector for one `[[external]]` of the setup, and [`protocol_schema()`] is the JSON schema of the messages it exchanges with its process.

mod depth;
mod document;
mod editor;
mod error;
mod evaluate;
pub mod external;
mod fingerprint;
pub mod pack;
mod source;
#[cfg(test)]
mod testlog;
mod text;

pub use editor::{CONFIG_SCHEMA_PATH, PACK_SCHEMA_PATH, starter};
pub use error::ConfigError;
pub use evaluate::{CONFIG_PATH, PACKS_PATH, evaluate};
pub use external::protocol::{PROTOCOL, schema as protocol_schema};
pub use external::{External, ExternalProject};
pub use fingerprint::fingerprint;
pub use pack::{PackError, builtin_packs, read_pack};
