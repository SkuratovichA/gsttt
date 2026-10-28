//! Answering a call: what each tool asks of the source and of the queries.

use crate::arguments::Call;
use crate::error::ToolError;
use crate::source::IndexSource;

/// Answers `call` from `source` as the text of one result.
///
/// # Errors
///
/// Whatever stops the call: a path the source refuses, a failed refresh, a source that is down.
pub(crate) fn run<S: IndexSource>(_source: &S, _call: Call) -> Result<String, ToolError> {
    Err(ToolError::Unanswered)
}
