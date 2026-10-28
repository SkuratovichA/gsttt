//! What the tests read from the log.

use std::io;
use std::sync::{Arc, Mutex};

/// The lines `run` writes to the log on this thread, each as its level, its target and its text: ` INFO gsttt_config::lua: hello`. A subscriber is local to a thread, so a line another thread writes is not here.
pub(crate) fn capture(run: impl FnOnce()) -> Vec<String> {
    let buffer = Buffer::default();
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    tracing::subscriber::with_default(subscriber, run);
    let bytes = buffer.0.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A log writer that keeps what it is given.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
