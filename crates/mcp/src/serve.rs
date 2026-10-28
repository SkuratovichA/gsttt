//! Serving a connection: the entry point of the binary, and of the tests that drive the server over a pipe.

use rmcp::ServiceExt;
use rmcp::service::{QuitReason, ServerInitializeError};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::handler::Server;
use crate::source::IndexSource;

/// Why a session ended in an error. A client that hangs up is not one.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    /// The client did not open the session as the protocol has it.
    #[error("the session did not start: {reason}")]
    Start {
        /// What went wrong, in the words of the protocol library.
        reason: String,
    },
    /// The task that runs the session failed, which a panic in the server would do.
    #[error("the task that runs the session failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Serves the protocol on the standard streams until the client closes its end, and returns then.
///
/// # Errors
///
/// [`ServeError`] when the session could not start, or when its task failed.
pub async fn serve<S: IndexSource>(source: S) -> Result<(), ServeError> {
    let (input, output) = rmcp::transport::stdio();
    serve_on(source, input, output).await
}

/// Serves the protocol over `input` and `output` until `input` ends, and returns then.
///
/// Nothing but protocol messages is written to `output`. The binary passes the standard streams through [`serve`];
/// the tests pass the two ends of a pipe.
///
/// # Errors
///
/// [`ServeError`] when the session could not start, or when its task failed.
pub async fn serve_on<S, R, W>(source: S, input: R, output: W) -> Result<(), ServeError>
where
    S: IndexSource,
    R: AsyncRead + Send + Unpin + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let running = match Server::new(source).serve((input, output)).await {
        Ok(running) => running,
        // A client that hangs up before it says hello has asked for nothing, so there is nothing to report.
        Err(ServerInitializeError::ConnectionClosed(_)) => return Ok(()),
        Err(error) => {
            return Err(ServeError::Start {
                reason: error.to_string(),
            });
        }
    };
    match running.waiting().await? {
        QuitReason::JoinError(error) => Err(ServeError::Task(error)),
        // The input ended, or the session was stopped from outside: both end it as it should end.
        _ => Ok(()),
    }
}
