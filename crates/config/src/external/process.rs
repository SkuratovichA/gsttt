//! The child process of an external connector: started on the first request and spoken to one line at a time.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use gsttt_core::connector::Provide;
use gsttt_core::fact::{FileFacts, Import};
use serde_json::Map;

use super::protocol::{PROTOCOL, Request, Response, WireResolution};

/// The longest line the process may send. A message holds the facts of one file, so this is far above any real one, and it stops a process that never ends its line from filling the memory.
const MAX_LINE: u64 = 64 * 1024 * 1024;

/// How long a process gets to leave on its own after `done` before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// How often the exit of the process is looked for while it is given its grace.
const EXIT_POLL: Duration = Duration::from_millis(5);

/// Why a request got no usable answer.
#[derive(Debug, thiserror::Error)]
pub(super) enum Failure {
    #[error("could not run `{command}`: {source}")]
    Start { command: String, source: io::Error },
    #[error("the process exited")]
    Exited,
    #[error("no answer within {0:?}")]
    Timeout(Duration),
    #[error("answered out of turn: {got} where {due} was due")]
    OutOfTurn { got: String, due: String },
    #[error("sent a line that is not a message ({error}): {line}")]
    Unparseable {
        line: String,
        error: serde_json::Error,
    },
    #[error("could not talk to the process: {0}")]
    Io(io::Error),
    #[error("could not write the request: {0}")]
    Encode(serde_json::Error),
}

/// A request in the wire form, and where the line the process answers with goes. `done` has no answer.
struct Exchange {
    line: String,
    reply: Option<SyncSender<io::Result<String>>>,
}

/// A running process and the thread that talks to it.
#[derive(Debug)]
pub(super) struct Process {
    child: Child,
    /// The way to the thread that owns the pipes of the process; `None` once the process is stopped.
    requests: Option<Sender<Exchange>>,
    timeout: Duration,
    provides: Vec<Provide>,
    stopped: bool,
}

impl Process {
    /// Starts `program` in `root` and runs the handshake: `open`, answered by `ready`. The lines of its standard error go to the log under `name`.
    pub(super) fn start(
        name: &'static str,
        program: &str,
        arguments: &[String],
        root: &Path,
        timeout: Duration,
    ) -> Result<Self, Failure> {
        let mut child = Command::new(program)
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Failure::Start {
                command: command_line(program, arguments),
                source,
            })?;
        let (requests, exchanges) = mpsc::channel();
        let threads = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
            (Some(stdin), Some(stdout), Some(stderr)) => {
                spawn_threads(name, stdin, stdout, stderr, exchanges)
            }
            _ => Err(io::Error::other("the standard streams were not piped")),
        };
        if let Err(source) = threads {
            stop(&mut child);
            return Err(Failure::Start {
                command: command_line(program, arguments),
                source,
            });
        }
        let mut process = Self {
            child,
            requests: Some(requests),
            timeout,
            provides: Vec::new(),
            stopped: false,
        };
        match process.open() {
            Ok(provides) => {
                process.provides = provides;
                Ok(process)
            }
            Err(failure) => {
                process.kill();
                Err(failure)
            }
        }
    }

    /// What the process said it provides in `ready`.
    pub(super) fn provides(&self) -> &[Provide] {
        &self.provides
    }

    /// `open`: the answer is `ready`, and its provides come back.
    fn open(&mut self) -> Result<Vec<Provide>, Failure> {
        let options = Map::new();
        let request = Request::Open {
            protocol: PROTOCOL,
            root: "",
            options: &options,
        };
        match self.round_trip(&request)? {
            Response::Ready { provides, .. } => Ok(provides),
            other => Err(out_of_turn(&other, "ready")),
        }
    }

    /// `parse`: the answer is the `facts` of the same path.
    pub(super) fn parse(&mut self, path: &str, text: &str) -> Result<FileFacts, Failure> {
        match self.round_trip(&Request::Parse { path, text })? {
            Response::Facts {
                path: answered,
                facts,
            } if answered == path => Ok(facts),
            other => Err(out_of_turn(&other, &format!("facts for `{path}`"))),
        }
    }

    /// `resolve`: the answer is a `resolution`.
    pub(super) fn resolve(
        &mut self,
        from: &str,
        import: &Import,
    ) -> Result<WireResolution, Failure> {
        match self.round_trip(&Request::Resolve { from, import })? {
            Response::Resolution { resolution } => Ok(resolution),
            other => Err(out_of_turn(&other, "resolution")),
        }
    }

    /// Stops the process at once, for a request it did not answer.
    pub(super) fn kill(&mut self) {
        self.requests = None;
        stop(&mut self.child);
        self.stopped = true;
    }

    /// Writes `request` and waits for the one line that answers it.
    fn round_trip(&mut self, request: &Request<'_>) -> Result<Response, Failure> {
        let mut line = serde_json::to_string(request).map_err(Failure::Encode)?;
        line.push('\n');
        let requests = self.requests.as_ref().ok_or(Failure::Exited)?;
        let (reply, answer) = mpsc::sync_channel(1);
        requests
            .send(Exchange {
                line,
                reply: Some(reply),
            })
            .map_err(|_| Failure::Exited)?;
        let line = match answer.recv_timeout(self.timeout) {
            Ok(read) => read.map_err(from_io)?,
            Err(RecvTimeoutError::Timeout) => return Err(Failure::Timeout(self.timeout)),
            Err(RecvTimeoutError::Disconnected) => return Err(Failure::Exited),
        };
        serde_json::from_str(&line).map_err(|error| Failure::Unparseable {
            line: line.chars().take(200).collect(),
            error,
        })
    }
}

impl Drop for Process {
    /// Ends the run: `done`, then the pipes close, and a process that does not leave within its grace is killed.
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        if let Some(requests) = self.requests.take()
            && let Ok(mut line) = serde_json::to_string(&Request::Done)
        {
            line.push('\n');
            requests.send(Exchange { line, reply: None }).ok();
        }
        let deadline = Instant::now() + EXIT_GRACE;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            thread::sleep(EXIT_POLL);
        }
        stop(&mut self.child);
    }
}

/// Starts the thread that owns the pipes of the process and the thread that logs its standard error. They end on their own once the process is gone.
fn spawn_threads(
    name: &'static str,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    exchanges: Receiver<Exchange>,
) -> io::Result<()> {
    thread::Builder::new()
        .name(format!("{name}-requests"))
        .spawn(move || drive(stdin, BufReader::new(stdout), exchanges))?;
    thread::Builder::new()
        .name(format!("{name}-stderr"))
        .spawn(move || forward(name, BufReader::new(stderr)))?;
    Ok(())
}

/// Writes each request and reads its answer, one at a time. A process that stops reading or answering blocks this thread and nobody else: the caller gives up on its timeout and kills the process, which frees the thread.
fn drive(mut stdin: ChildStdin, mut stdout: BufReader<ChildStdout>, exchanges: Receiver<Exchange>) {
    for Exchange { line, reply } in exchanges {
        let written = stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush());
        let Some(reply) = reply else {
            continue;
        };
        if reply
            .send(written.and_then(|()| read_line(&mut stdout)))
            .is_err()
        {
            return;
        }
    }
}

/// Sends what the process writes to standard error to the log until it closes the pipe.
fn forward(name: &str, mut stderr: BufReader<ChildStderr>) {
    let mut line = Vec::new();
    while stderr
        .by_ref()
        .take(MAX_LINE)
        .read_until(b'\n', &mut line)
        .is_ok_and(|read| read > 0)
    {
        tracing::warn!(
            connector = name,
            "{}",
            String::from_utf8_lossy(&line).trim_end()
        );
        line.clear();
    }
}

/// The next line of the process, without its newline.
fn read_line(reader: &mut BufReader<ChildStdout>) -> io::Result<String> {
    let mut line = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_LINE)
        .read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
        Ok(String::from_utf8_lossy(&line).into_owned())
    } else if read as u64 >= MAX_LINE {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a line longer than {MAX_LINE} bytes"),
        ))
    } else {
        // The pipe closed before the line ended.
        Err(io::ErrorKind::UnexpectedEof.into())
    }
}

/// Kills the process and collects it. Both fail only when it is gone already, which is the point.
fn stop(child: &mut Child) {
    child.kill().ok();
    child.wait().ok();
}

/// A pipe that closed under a read or a write is a process that left.
fn from_io(error: io::Error) -> Failure {
    if matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
    ) {
        Failure::Exited
    } else {
        Failure::Io(error)
    }
}

fn out_of_turn(got: &Response, due: &str) -> Failure {
    Failure::OutOfTurn {
        got: got.describe(),
        due: due.to_owned(),
    }
}

fn command_line(program: &str, arguments: &[String]) -> String {
    std::iter::once(program)
        .chain(arguments.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}
