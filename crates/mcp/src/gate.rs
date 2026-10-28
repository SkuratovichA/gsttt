//! One call into the source at a time, on a blocking thread.
//!
//! A refresh can take minutes on the first index of a large repository. Two at once would build the same index twice
//! and fight over the file that holds it, and one on the thread that reads the transport would stop the server from
//! answering anything until it ended. So a call waits for its turn, and then runs on a thread of the blocking pool.

use std::sync::Arc;

use tokio::sync::Mutex;

/// The turn to call the source: at most one piece of work runs behind it.
#[derive(Debug, Default)]
pub(crate) struct Gate(Arc<Mutex<()>>);

/// The work behind the gate ended without a result: it panicked, or the runtime stopped before it could run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stopped;

impl Gate {
    /// Waits for the turn, then runs `work` on a blocking thread and returns what it returned.
    ///
    /// The turn belongs to the work and not to the caller. It is handed on when `work` ends, even if the caller has
    /// stopped waiting for it by then, so a call that is dropped halfway cannot let the next one start beside it.
    ///
    /// # Errors
    ///
    /// [`Stopped`] when `work` panicked, or when the runtime shut down before it ran. Either way the turn is handed on.
    pub(crate) async fn run<T, F>(&self, work: F) -> Result<T, Stopped>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let turn = Arc::clone(&self.0).lock_owned().await;
        tokio::task::spawn_blocking(move || {
            let _turn = turn;
            work()
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "the work behind the gate did not finish");
            Stopped
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

    /// How long a test waits for a turn that should be handed on at once, before it calls the gate stuck.
    const PATIENCE: Duration = Duration::from_secs(20);

    #[tokio::test]
    async fn work_returns_what_it_computed() {
        let gate = Gate::default();

        assert_eq!(gate.run(|| 6 * 7).await, Ok(42));
    }

    #[tokio::test]
    async fn the_turn_stays_with_work_whose_caller_gave_up() {
        let gate = Gate::default();
        let (started, has_started) = oneshot::channel();
        let (release, released) = mpsc::channel::<()>();

        // The first caller walks away as soon as its work is running, and that work goes on until it is released.
        tokio::select! {
            biased;
            _ = gate.run(move || {
                started.send(()).ok();
                released.recv().ok();
            }) => panic!("the work cannot end before it is released"),
            _ = has_started => {}
        }

        // The second caller finds the turn taken: one poll leaves it waiting.
        let mut second = Box::pin(gate.run(|| 7));
        let polled = tokio::select! {
            biased;
            reply = &mut second => Some(reply),
            () = std::future::ready(()) => None,
        };
        assert_eq!(
            polled, None,
            "the second caller waits for work still running"
        );

        release.send(()).unwrap();
        let reply = tokio::time::timeout(PATIENCE, second)
            .await
            .expect("the turn is handed on when the work ends");
        assert_eq!(reply, Ok(7));
    }

    #[tokio::test]
    async fn a_panic_ends_the_call_and_hands_the_turn_on() {
        let gate = Gate::default();

        let broken = gate.run(|| -> u32 { panic!("the work broke") }).await;
        assert_eq!(broken, Err(Stopped));

        let next = tokio::time::timeout(PATIENCE, gate.run(|| 5))
            .await
            .expect("the turn is handed on after a panic");
        assert_eq!(next, Ok(5));
    }
}
