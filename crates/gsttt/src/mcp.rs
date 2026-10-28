//! `gsttt mcp`: answers the questions of an agent over the Model Context Protocol, on the standard streams.

use std::future::Future;

use camino::Utf8PathBuf;
use gsttt_core::engine::changes::Change;
use gsttt_core::{Loaded, Target};
use gsttt_mcp::{IndexSource, ServeError, SourceError};

use crate::cli::Flags;
use crate::error::CliError;
use crate::repository::{Repository, RepositoryError};
use crate::root;
use crate::streams::Outcome;

/// Serves the repository around the directory the command started in, until the client closes its input.
pub(crate) fn run(flags: &Flags) -> Result<Outcome, CliError> {
    let start = root::start(flags.directory.as_deref())?;
    block_on(gsttt_mcp::serve(Source { start }))?;
    Ok(Outcome::Answered)
}

/// Runs `session` to its end on a runtime of its own, and returns how it ended.
///
/// The runtime is dropped without waiting for its blocking threads. One of them may still be running a call that refreshes the index, and one may still be reading stdin when the session ends before the client has closed it, so that waiting would hold the process. The index is written atomically, so a refresh left behind loses nothing.
fn block_on(session: impl Future<Output = Result<(), ServeError>>) -> Result<(), CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| CliError::Runtime { source })?;
    let served = runtime.block_on(session);
    runtime.shutdown_background();
    Ok(served?)
}

/// The repository around the directory the command started in. Every call opens it again, so that an edit to the config or to the trust store takes effect in a session that is already running.
#[derive(Debug)]
struct Source {
    start: Utf8PathBuf,
}

impl IndexSource for Source {
    fn refreshed(&self) -> Result<Loaded, SourceError> {
        Ok(Repository::open(&self.start)?.refreshed()?.loaded)
    }

    fn stored(&self) -> Result<Option<Loaded>, SourceError> {
        Ok(Repository::open(&self.start)?.stored()?)
    }

    /// Paths are read from the root, because the server has no directory a model could mean.
    fn path(&self, argument: &str) -> Result<Target, SourceError> {
        let repository = Repository::open(&self.start)?;
        Ok(repository.argument(repository.root(), argument)?)
    }

    fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, SourceError> {
        Ok(Repository::open(&self.start)?.changed(base)?)
    }
}

impl From<RepositoryError> for SourceError {
    /// The failure as the sentence the command line prints, which the model reads as it is.
    fn from(error: RepositoryError) -> Self {
        Self::new(error.to_string())
    }
}
