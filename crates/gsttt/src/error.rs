//! What a command can fail with, and how a failure is told on stderr.

use std::io;

use camino::Utf8PathBuf;
use gsttt_core::engine::changes::ChangesError;
use gsttt_core::engine::refresh::IndexError;
use gsttt_core::path::OutsideRoot;
use gsttt_mcp::ServeError;
use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme, Report};

use crate::repository::RepositoryError;
use crate::root::RootError;
use crate::streams::{Io, Outcome, reader_left};
use crate::trust::StoreError;

/// Why a command did not answer. The message names what was expected, and the `help:` line, where there is one,
/// says what the user can do.
#[derive(Debug, thiserror::Error, Diagnostic)]
pub(crate) enum CliError {
    #[error("cannot write the answer to stdout: {source}")]
    Write { source: io::Error },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Root(#[from] RootError),

    #[error(transparent)]
    #[diagnostic(help(
        "a path is read from the current directory, or from the one `-C` names, and has to name a file inside the repository"
    ))]
    Path(#[from] OutsideRoot),

    /// A failure of the repository that the command line has nothing to add to. The ones it words itself have
    /// variants of their own, and [`CliError::from`] sorts a [`RepositoryError`] into them.
    #[error(transparent)]
    Repository(RepositoryError),

    #[error("`--changed` needs a git repository, and `{root}` is in none")]
    #[diagnostic(help("name the files instead, as in `gsttt impact path/to/file.ts`"))]
    ChangedNeedsGit { root: Utf8PathBuf },

    #[error(transparent)]
    #[diagnostic(help(
        "`--base` takes a branch, a tag or a commit of this repository, and has to share history with HEAD"
    ))]
    Base(ChangesError),

    #[error("cannot print the answer as JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("there is no index yet for `{root}`")]
    #[diagnostic(help("run `gsttt index` to build one"))]
    NoIndex { root: Utf8PathBuf },

    #[error(
        "trusting an external connector needs a terminal, and stdin and stderr are not both one"
    )]
    #[diagnostic(help(
        "run `gsttt config trust` yourself in a terminal: a program that drives gsttt cannot trust on your behalf"
    ))]
    TrustNeedsTerminal,

    /// The key of an external cannot be made, which the person has to fix before there is anything to trust.
    #[error("{0}")]
    Key(String),

    #[error(transparent)]
    Store(#[from] StoreError),

    /// A file that `gsttt init` writes cannot be written, or the folder above it cannot be made.
    #[error("cannot write `{path}`")]
    #[diagnostic(help(
        "`gsttt init` writes into `.gsttt/` at the root of the repository; check that it can be written to"
    ))]
    Init {
        path: Utf8PathBuf,
        source: io::Error,
    },

    #[error(transparent)]
    Serve(#[from] ServeError),

    #[error("cannot start the runtime of the MCP server: {source}")]
    Runtime { source: io::Error },

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

    #[error(
        "`gsttt mcp` takes none of `--json`, `--all` and `--no-refresh`, and `{flag}` was given"
    )]
    #[diagnostic(help(
        "drop `{flag}`: the server answers in protocol messages, a tool that lists entries takes `all` as an argument, and every call refreshes the index first"
    ))]
    McpTakesNoFlag { flag: &'static str },
}

impl From<RepositoryError> for CliError {
    /// Sorts a failure of the repository into the variant that words it for the command line.
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Root(error) => Self::Root(error),
            RepositoryError::Path(error) => Self::Path(error),
            RepositoryError::NoGit { root } => Self::ChangedNeedsGit { root },
            RepositoryError::Index(IndexError::Changes(
                error @ (ChangesError::Revision { .. } | ChangesError::NoMergeBase { .. }),
            )) => Self::Base(error),
            other => Self::Repository(other),
        }
    }
}

impl CliError {
    /// How the command ends: 1 when it could not answer, 2 when it cannot run as given.
    pub(crate) const fn outcome(&self) -> Outcome {
        match self {
            Self::Write { .. }
            | Self::Json(_)
            | Self::Repository(_)
            | Self::NoIndex { .. }
            | Self::Key(_)
            | Self::Store(_)
            | Self::Init { .. }
            | Self::Serve(_)
            | Self::Runtime { .. }
            | Self::Root(RootError::Workspace(_) | RootError::CurrentDirectory { .. }) => {
                Outcome::Failed
            }
            Self::Root(_)
            | Self::Path(_)
            | Self::ChangedNeedsGit { .. }
            | Self::Base(_)
            | Self::EmptyQuery
            | Self::BaseNeedsChanged
            | Self::NoRefreshWithIndex
            | Self::TrustNeedsTerminal
            | Self::McpTakesNoFlag { .. } => Outcome::Usage,
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
