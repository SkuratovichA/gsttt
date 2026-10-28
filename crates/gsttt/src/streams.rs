//! The streams a command writes to, and how a command ends.

use std::fmt;
use std::io::{self, Write};

use anstream::stream::RawStream;
use anstream::{AutoStream, ColorChoice};

use crate::error::CliError;

/// How a command ended, which `main` turns into the exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The command answered, including an answer that found nothing: exit 0.
    Answered,
    /// The command could not answer: exit 1.
    Failed,
    /// The command cannot run as given: exit 2.
    Usage,
}

impl Outcome {
    /// The exit code of the process.
    pub const fn code(self) -> u8 {
        match self {
            Self::Answered => 0,
            Self::Failed => 1,
            Self::Usage => 2,
        }
    }
}

/// The output streams of the process, replaced by buffers in tests. Text is written to them with its ANSI styling
/// in place: the process wraps its streams so that the styling reaches a terminal and nothing else.
pub struct Io<'a> {
    /// The answer, and nothing else.
    pub stdout: &'a mut dyn Write,
    /// Errors, usage and logs.
    pub stderr: &'a mut dyn Write,
}

impl fmt::Debug for Io<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Io").finish_non_exhaustive()
    }
}

impl<'a> Io<'a> {
    /// Streams to write the answer and the errors to.
    pub fn new(stdout: &'a mut dyn Write, stderr: &'a mut dyn Write) -> Self {
        Self { stdout, stderr }
    }

    /// Writes `text` to stdout and flushes it, so that the answer is out before the process ends.
    pub(crate) fn answer(&mut self, text: &[u8]) -> Result<(), CliError> {
        self.stdout
            .write_all(text)
            .and_then(|()| self.stdout.flush())
            .map_err(|source| CliError::Write { source })
    }

    /// Writes `text` to stderr. A failure to write there has nowhere to be told, so it is dropped.
    pub(crate) fn complain(&mut self, text: &[u8]) {
        self.stderr
            .write_all(text)
            .and_then(|()| self.stderr.flush())
            .unwrap_or_else(|error| tracing::debug!(%error, "cannot write to stderr"));
    }
}

/// The process's stdout. Styling written to it reaches a terminal and nothing else.
pub fn stdout() -> AutoStream<io::Stdout> {
    wrap(io::stdout())
}

/// The process's stderr, wrapped like [`stdout`].
pub fn stderr() -> AutoStream<io::Stderr> {
    wrap(io::stderr())
}

/// Whether styling written to `stream` is shown. The logs are written past the wrapper, so they ask this to decide
/// whether to style themselves.
pub fn shows_styling<S: RawStream>(stream: &AutoStream<S>) -> bool {
    stream.current_choice() != ColorChoice::Never
}

/// Colour is for a terminal: on one, `anstream` decides from `NO_COLOR`, `CLICOLOR` and `TERM`; anywhere else the
/// styling is stripped. `anstream` alone would let `CLICOLOR_FORCE` push escape sequences into a pipe, and a pipe is
/// read by a program that does not expect them.
fn wrap<S: RawStream>(stream: S) -> AutoStream<S> {
    let choice = if stream.is_terminal() {
        AutoStream::choice(&stream)
    } else {
        ColorChoice::Never
    };
    AutoStream::new(stream, choice)
}

/// Whether `error` says the reader of a stream went away, which is how `gsttt … | head` ends.
pub(crate) fn reader_left(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::BrokenPipe
}
