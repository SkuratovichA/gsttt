//! Logs: the `tracing` events of every crate, on stderr, filtered by `GSTTT_LOG`.

use std::io;

use tracing::Subscriber;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{EnvFilter, fmt};

/// The variable that filters the logs, in the syntax of [`EnvFilter`]: a level such as `debug`, or targets with
/// levels such as `gsttt_core=trace,warn`. Without it only errors show.
pub const FILTER_VARIABLE: &str = "GSTTT_LOG";

/// Starts the logs of the process: lines on stderr, filtered by `GSTTT_LOG` and styled when `styled`.
///
/// Logs never go to stdout, which carries the answer and nothing else. Calling this twice keeps the first logger.
pub fn init(styled: bool) {
    let directives = std::env::var(FILTER_VARIABLE).unwrap_or_default();
    let logger = subscriber(&directives, styled, io::stderr);
    // This fails only when a logger is set already, and that one stays.
    let _ = tracing::subscriber::set_global_default(logger);
}

/// A logger for `directives`, which are the text of `GSTTT_LOG`. Directives that cannot be read are skipped, and
/// when none is left only errors show. Lines carry no time, so that the same run logs the same text.
fn subscriber<W>(directives: &str, styled: bool, writer: W) -> impl Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::ERROR.into())
        .parse_lossy(directives);
    fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(styled)
        .without_time()
        .finish()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// A writer that keeps what it is given, for the logger to write to.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl MakeWriter<'_> for Sink {
        type Writer = Self;

        fn make_writer(&self) -> Self {
            self.clone()
        }
    }

    /// What the logger writes for one event at each of three levels.
    fn logged(directives: &str, styled: bool) -> String {
        let sink = Sink::default();
        tracing::subscriber::with_default(subscriber(directives, styled, sink.clone()), || {
            tracing::error!("an error");
            tracing::warn!("a warning");
            tracing::debug!("a detail");
        });
        String::from_utf8(sink.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn only_errors_show_without_a_filter() {
        let text = logged("", false);
        assert!(text.contains("an error"), "{text}");
        assert!(
            !text.contains("a warning") && !text.contains("a detail"),
            "{text}"
        );
    }

    #[test]
    fn the_filter_sets_the_level() {
        let text = logged("warn", false);
        assert!(
            text.contains("an error") && text.contains("a warning"),
            "{text}"
        );
        assert!(!text.contains("a detail"), "{text}");
        let text = logged("debug", false);
        assert!(text.contains("a detail"), "{text}");
    }

    #[test]
    fn a_directive_that_cannot_be_read_leaves_only_errors() {
        let text = logged("not a=level=at all", false);
        assert!(text.contains("an error"), "{text}");
        assert!(!text.contains("a warning"), "{text}");
    }

    #[test]
    fn lines_are_styled_only_when_asked_to() {
        assert!(logged("", true).contains('\u{1b}'));
        assert!(!logged("", false).contains('\u{1b}'));
    }

    #[test]
    fn lines_carry_no_time() {
        assert_eq!(logged("", false), "ERROR gsttt::logging::tests: an error\n");
    }
}
