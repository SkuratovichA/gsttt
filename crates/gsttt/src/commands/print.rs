//! How an answer reaches stdout, and how one that is not an answer reaches stderr.

use gsttt_core::query::{Answer, Entries};

use crate::cli::Flags;
use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// Prints `answer` on stdout: its text with the lists cut as `--all` says, or, under `--json`, one JSON document
/// that holds every entry whatever `--all` says.
pub(super) fn answer<A: Answer>(
    answer: &A,
    flags: &Flags,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    let mut text = if flags.json {
        serde_json::to_string(answer)?
    } else {
        answer.text(Entries::from_flag(flags.all))
    };
    // The text of an answer ends at its last line, with no newline after it.
    text.push('\n');
    io.answer(text.as_bytes())?;
    Ok(Outcome::Answered)
}

/// Prints `answer` when `held` says the index holds the subject it was asked about. When it does not, the question
/// has no answer: its text goes to stderr, stdout stays empty even under `--json`, and the run fails.
pub(super) fn answer_if<A: Answer>(
    held: bool,
    answered: &A,
    flags: &Flags,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    if held {
        return answer(answered, flags, io);
    }
    let mut text = answered.text(Entries::Capped);
    text.push('\n');
    io.complain(text.as_bytes());
    Ok(Outcome::Failed)
}
