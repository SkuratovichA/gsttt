//! What a command can fail with, and how a failure is told on stderr.

use std::io;

use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Report};

use crate::streams::{Io, Outcome, reader_left};

/// Why a command did not answer. The message names what was expected, and the `help:` line, where there is one,
/// says what the user can do.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub(crate) enum CliError {
    #[error("cannot write the answer to stdout: {source}")]
    Write { source: io::Error },

    #[error("`gsttt {command}` is not built yet")]
    NotBuilt { command: &'static str },

    #[error("the contract query is empty")]
    #[diagnostic(help(
        "ask for a contract key such as `http GET /bookings/{{id}}`, or for part of one"
    ))]
    EmptyQuery,

    #[error("`--base` needs `--changed`")]
    #[diagnostic(help(
        "`--base <rev>` says what `--changed` compares with: `gsttt impact --changed --base <rev>`"
    ))]
    BaseNeedsChanged,

    #[error("`index` always refreshes the index, so it cannot run with `--no-refresh`")]
    #[diagnostic(help("drop `--no-refresh`"))]
    NoRefreshWithIndex,
}

impl CliError {
    /// How the command ends: 1 when it could not answer, 2 when it cannot run as given.
    pub(crate) const fn outcome(&self) -> Outcome {
        match self {
            Self::Write { .. } | Self::NotBuilt { .. } => Outcome::Failed,
            Self::EmptyQuery | Self::BaseNeedsChanged | Self::NoRefreshWithIndex => Outcome::Usage,
        }
    }

    /// Tells the failure on stderr, through miette, and returns how the command ends.
    pub(crate) fn report(self, io: &mut Io<'_>) -> Outcome {
        let outcome = self.outcome();
        // A reader that went away (`gsttt … | head`) asked for no more output, errors included.
        if matches!(&self, Self::Write { source } if reader_left(source)) {
            return outcome;
        }
        io.complain(render(&Report::new(self)).as_bytes());
        outcome
    }
}

/// The report as text: the message, its causes and the help line. Lines are never wrapped, so that the same
/// failure reads the same in a pipe and on a terminal of any width.
fn render(report: &Report) -> String {
    let handler =
        GraphicalReportHandler::new_themed(GraphicalTheme::unicode()).with_wrap_lines(false);
    let mut text = String::new();
    match handler.render_report(&mut text, report.as_ref()) {
        Ok(()) => text,
        Err(_) => format!("{report}\n"),
    }
}
