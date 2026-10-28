//! The `gsttt` command line: its commands, their arguments and what they print.
//!
//! `main.rs` only reads the command line, installs the process-wide handlers and turns an [`Outcome`] into an exit
//! code. The commands live here, so that a test can run one over buffers.

pub mod cli;
mod commands;
mod error;
pub mod hook;
pub mod logging;
pub mod panics;
pub mod registry;
pub mod root;
pub mod streams;

pub use cli::{Cli, parse};
pub use commands::run;
pub use streams::{Io, Outcome};
