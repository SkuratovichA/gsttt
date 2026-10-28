//! `gsttt schema`: the JSON Schemas that gsttt generates for other programs to check themselves against.

use gsttt_config::protocol_schema;
use serde_json::Value;

use super::print;
use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// `gsttt schema external`: prints the JSON Schema of the messages that gsttt and an external connector exchange.
///
/// It needs no repository, so it finds no root and reads no config, and it prints the same schema under `--json`, because a schema is JSON already.
pub(super) fn external(io: &mut Io<'_>) -> Result<Outcome, CliError> {
    io.answer(print::indented(&Value::from(protocol_schema())).as_bytes())?;
    Ok(Outcome::Answered)
}
