//! The gsttt MCP server.
//!
//! The server offers the questions of the command line as tools. It does not know where the index comes from: the binary hands it an [`IndexSource`], and every tool call goes through that trait. The answers are the text the command line prints, produced by the same code in `gsttt-core`.

mod arguments;
mod ask;
mod error;
mod gate;
mod handler;
mod serve;
mod source;
mod tools;

pub use serve::{ServeError, serve, serve_on};
pub use source::{IndexSource, SourceError};
