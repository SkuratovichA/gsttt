//! The gsttt MCP server.
//!
//! The server offers the questions of the command line as tools. It does not know where the index comes from:
//! the binary hands it an [`IndexSource`], and every tool call goes through that trait.

mod source;

pub use source::{IndexSource, Loaded, SourceError, Target};
