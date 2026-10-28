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

/// What a model can do about a config that does not read, which the command line leaves to the person at the terminal. The repository is opened again on every call, so a fix takes effect on the next one.
const FIX_THE_CONFIG: &str = "Fix the config and ask again: the server reads it again on every call, so it needs no restart.";

impl From<RepositoryError> for SourceError {
    /// The failure as the sentence the command line prints, which the model reads as it is, and after a failure of the config the line that says what to do about it. No arm is a wildcard, so a new failure of the repository has to be sorted here.
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Config(_)
            | RepositoryError::External(_)
            | RepositoryError::Options(_) => Self::new(format!("{error}\n{FIX_THE_CONFIG}")),
            RepositoryError::Root(_)
            | RepositoryError::Packs(_)
            | RepositoryError::Path(_)
            | RepositoryError::NoGit { .. }
            | RepositoryError::Index(_) => Self::new(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use camino::Utf8PathBuf;
    use gsttt_config::ConfigError;
    use gsttt_core::connector::ConnectorError;
    use gsttt_core::engine::changes::Change;
    use gsttt_core::{Loaded, Target};
    use gsttt_mcp::{IndexSource, SourceError, serve_on};
    use serde_json::json;
    use tokio::io::{AsyncWriteExt, DuplexStream};

    use super::{RepositoryError, block_on};

    /// How long the session may take to end once its input has closed. The server gives the calls in progress a few seconds to answer, and the refresh below would take thirty.
    const ENDS_WITHIN: Duration = Duration::from_secs(10);

    /// A source whose refresh says that it has started, and then takes longer than anything in the test waits for.
    #[derive(Debug)]
    struct Slow {
        started: mpsc::Sender<()>,
    }

    impl IndexSource for Slow {
        fn refreshed(&self) -> Result<Loaded, SourceError> {
            self.started
                .send(())
                .map_err(|_| SourceError::new("the test stopped listening"))?;
            thread::sleep(Duration::from_secs(30));
            Err(SourceError::new("the refresh took thirty seconds"))
        }

        fn stored(&self) -> Result<Option<Loaded>, SourceError> {
            Ok(None)
        }

        fn path(&self, _argument: &str) -> Result<Target, SourceError> {
            Err(SourceError::new("the question reads no path"))
        }

        fn changed(&self, _base: Option<&str>) -> Result<Vec<Change>, SourceError> {
            Ok(Vec::new())
        }
    }

    /// What an agent does: opens the session, asks a question that needs a refresh, waits until the refresh is running, and closes its end.
    fn close_during_a_refresh(end: DuplexStream, refresh: &mpsc::Receiver<()>) {
        let messages = [
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "a test", "version": "0" },
                },
            }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "coverage", "arguments": {} },
            }),
        ];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut end = end;
            for message in messages {
                end.write_all(format!("{message}\n").as_bytes())
                    .await
                    .unwrap();
            }
            refresh
                .recv_timeout(ENDS_WITHIN)
                .expect("the refresh started");
            drop(end);
        });
    }

    #[test]
    fn the_session_ends_when_its_input_closes_during_a_refresh() {
        let (client_end, server_end) = tokio::io::duplex(1 << 16);
        let (started, refresh) = mpsc::channel();
        let client = thread::spawn(move || close_during_a_refresh(client_end, &refresh));
        let (ended, session) = mpsc::channel();
        // The session runs on a thread of its own, so that a session that does not end fails the test and does not hang it.
        thread::spawn(move || {
            let (input, output) = tokio::io::split(server_end);
            ended
                .send(block_on(serve_on(Slow { started }, input, output)))
                .ok();
        });

        let result = session.recv_timeout(ENDS_WITHIN);

        client
            .join()
            .expect("the input closed while the refresh was running");
        let result = result.expect("the session ended within ten seconds of its input closing");
        assert!(result.is_ok(), "{result:?}");
    }

    /// What a model reads after a failure of the config, which the command line leaves to the person at the terminal.
    const FIX_THE_CONFIG: &str = "Fix the config and ask again: the server reads it again on every call, so it needs no restart.";

    /// A module that cannot read an option of its own.
    fn unreadable_option() -> ConnectorError {
        ConnectorError::Options {
            connector: "ts".to_owned(),
            message: "`no_such_option`: unknown field `no_such_option`".to_owned(),
        }
    }

    #[test]
    fn a_failure_of_the_config_ends_in_what_to_do() {
        // The evaluation of the config checks an `[[external]]` the way `External::new` does, so no file provokes the third failure and it is built here.
        let failures = [
            RepositoryError::Config(ConfigError::Invalid(
                ".gsttt/config.toml:3:4: unclosed table, expected `]`".to_owned(),
            )),
            RepositoryError::Options(vec![unreadable_option()]),
            RepositoryError::External(unreadable_option()),
        ];

        for failure in failures {
            let printed = failure.to_string();
            let message = SourceError::from(failure).message;
            assert_eq!(message, format!("{printed}\n{FIX_THE_CONFIG}"));
        }
    }

    #[test]
    fn a_failure_the_config_did_not_cause_keeps_the_sentence_of_the_command_line() {
        let failure = RepositoryError::NoGit {
            root: Utf8PathBuf::from("/work/project"),
        };
        let printed = failure.to_string();

        assert_eq!(SourceError::from(failure).message, printed);
    }
}
