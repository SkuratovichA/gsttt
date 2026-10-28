//! The child process of an external connector: started on the first request and spoken to one line at a time.

use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use gsttt_core::fact::{FileFacts, Import};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use serde_json::{Map, Value};

use super::protocol::{PROTOCOL, Request, Response, WireResolution, resolution_of};
use crate::text::{LOG_LINE_LIMIT, log_line};

/// The longest line the process may send. A message holds the facts of one file, so this is far above any real one, and it stops a process that never ends its line from filling the memory.
const MAX_LINE: u64 = 64 * 1024 * 1024;

/// How long a process gets to leave on its own after `done` before it is killed.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// How often the exit of the process is looked for while it is given its grace.
const EXIT_POLL: Duration = Duration::from_millis(5);

/// The variables a process gets from gsttt's own environment, when gsttt has them: what a program needs to find its tools and its home. Anything else, a secret among it, reaches the process only by name in the `env` of its declaration.
const INHERITED_ENV: [&str; 8] = [
    "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR",
];

/// What it takes to start the process of a connector.
#[derive(Debug)]
pub(super) struct Launch {
    /// The connector's name, for the log and the messages.
    pub(super) name: &'static str,
    pub(super) program: String,
    pub(super) arguments: Vec<String>,
    /// Names of the variables to pass on besides [`INHERITED_ENV`].
    pub(super) env: Vec<String>,
    /// What `open` carries as the options of the connector.
    pub(super) options: Map<String, Value>,
    /// Where the process runs: the repository root.
    pub(super) root: PathBuf,
    /// How long it gets to answer one request.
    pub(super) timeout: Duration,
}

impl Launch {
    /// The command line, as a message shows it.
    fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.arguments.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The part of gsttt's environment the process is given: the names of [`INHERITED_ENV`] and the listed ones, each with its value when gsttt has one.
    fn environment(&self) -> impl Iterator<Item = (&str, OsString)> {
        INHERITED_ENV
            .into_iter()
            .chain(self.env.iter().map(String::as_str))
            .filter_map(|name| env::var_os(name).map(|value| (name, value)))
    }
}

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
    stopped: bool,
}

impl Process {
    /// Starts the process in the repository root, with the environment of [`Launch::environment`] and nothing else, and runs the handshake: `open`, answered by `ready`. The lines of its standard error go to the log under its name.
    pub(super) fn start(launch: &Launch) -> Result<Self, Failure> {
        let mut child = Command::new(&launch.program)
            .args(&launch.arguments)
            .current_dir(&launch.root)
            .env_clear()
            .envs(launch.environment())
            // A group of its own, so that ending the process can end what it started along with it.
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Failure::Start {
                command: launch.command_line(),
                source,
            })?;
        let (requests, exchanges) = mpsc::channel();
        let threads = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
            (Some(stdin), Some(stdout), Some(stderr)) => {
                spawn_threads(launch.name, stdin, stdout, stderr, exchanges)
            }
            _ => Err(io::Error::other("the standard streams were not piped")),
        };
        if let Err(source) = threads {
            stop(&mut child);
            return Err(Failure::Start {
                command: launch.command_line(),
                source,
            });
        }
        let mut process = Self {
            child,
            requests: Some(requests),
            timeout: launch.timeout,
            stopped: false,
        };
        match process.open(&launch.options) {
            Ok(()) => Ok(process),
            Err(failure) => {
                process.kill();
                Err(failure)
            }
        }
    }

    /// `open`: the answer is `ready`.
    fn open(&mut self, options: &Map<String, Value>) -> Result<(), Failure> {
        let request = Request::Open {
            protocol: PROTOCOL,
            root: "",
            options,
        };
        match self.round_trip(&request)? {
            Response::Ready { .. } => Ok(()),
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

    /// `resolve`: the answer is the `resolution` of the same import of the same file.
    pub(super) fn resolve(
        &mut self,
        from: &str,
        import: &Import,
    ) -> Result<WireResolution, Failure> {
        match self.round_trip(&Request::Resolve { from, import })? {
            Response::Resolution {
                from: answered,
                import: repeated,
                resolution,
            } if answered == from && repeated == *import => Ok(resolution),
            other => Err(out_of_turn(&other, &resolution_of(from, import))),
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
    /// Ends the run: `done`, then the pipes close, and a process that does not leave within its grace is killed. What it started is killed in any case.
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
        while Instant::now() < deadline && !has_exited(&self.child) {
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
            .send(written.and_then(|()| read_line(&mut stdout, MAX_LINE)))
            .is_err()
        {
            return;
        }
    }
}

/// Sends what the process writes to standard error to the log until it closes the pipe: a warning for each line, made fit for a log by [`log_line`]. The process writes the text, so a line of any length must neither become a log line of that length nor sit in memory: what follows the first [`LOG_LINE_LIMIT`] bytes of a line is read and thrown away.
fn forward(name: &str, mut stderr: impl BufRead) {
    // One byte more than a log line holds, so that `log_line` can tell a line that ends there from one that goes on.
    let kept = LOG_LINE_LIMIT as u64 + 1;
    let mut line = Vec::new();
    loop {
        line.clear();
        match (&mut stderr).take(kept).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let ended = line.last() == Some(&b'\n');
        tracing::warn!(
            connector = name,
            "{}",
            log_line(&String::from_utf8_lossy(&line))
        );
        if !ended && stderr.skip_until(b'\n').is_err() {
            return;
        }
    }
}

/// The next line of the process, without its newline, read as far as `limit` bytes: a line that has not ended by then is an error, and nothing more of it is read.
fn read_line(reader: &mut impl BufRead, limit: u64) -> io::Result<String> {
    let mut line = Vec::new();
    let read = reader.by_ref().take(limit).read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
        Ok(String::from_utf8_lossy(&line).into_owned())
    } else if read as u64 >= limit {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a line longer than {limit} bytes"),
        ))
    } else {
        // The pipe closed before the line ended.
        Err(io::ErrorKind::UnexpectedEof.into())
    }
}

/// Whether the process has exited, without collecting it: a process that is collected gives up its id, and its id is the name of the group that [`stop`] signals.
fn has_exited(child: &Child) -> bool {
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    // An error means there is nothing to wait for, which is as good as an exit.
    !matches!(
        waitid(WaitId::Pid(Pid::from_child(child)), options),
        Ok(None)
    )
}

/// Kills the process and everything it started, then collects it. The group is signalled before the process is collected, while its id is still the process's own. Every step fails only when its target is gone already, which is the point.
///
/// A wrapper such as `sh -c`, `npx` or `gradle` leaves a child that holds the pipes of the process: killed alone, the wrapper would leave the threads that read the pipes waiting for ever, and the child running after gsttt.
fn stop(child: &mut Child) {
    kill_process_group(Pid::from_child(child), Signal::KILL).ok();
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

#[cfg(test)]
mod tests {
    use std::io::{self, BufReader};

    use super::*;
    use crate::testlog::capture;

    /// What `forward` logs for `stderr`, one entry per warning: its text, without the level, the target and the connector.
    fn forwarded(stderr: impl BufRead) -> Vec<String> {
        capture(|| forward("demo", stderr))
            .iter()
            .map(|line| {
                let text = line.split_once(": ").map_or("", |(_, text)| text);
                text.strip_suffix(" connector=\"demo\"")
                    .unwrap_or(text)
                    .to_owned()
            })
            .collect()
    }

    #[gsttt_scenario::scenario("config", "A noisy process cannot flood or forge the log")]
    #[test]
    fn a_noisy_process_cannot_flood_or_forge_the_log() {
        let long = "x".repeat(1_000_000);
        let stderr = format!("{long}\nfake\u{1b}[31m red\r\nplain\ttabbed");
        let lines = forwarded(stderr.as_bytes());
        assert_eq!(
            lines.len(),
            3,
            "{:?}",
            lines.iter().map(String::len).collect::<Vec<_>>()
        );
        assert_eq!(lines[0].len(), LOG_LINE_LIMIT);
        assert!(lines[0].starts_with("xxx") && lines[0].ends_with("x…"));
        assert_eq!(lines[1], "fake[31m red");
        assert_eq!(lines[2], "plain\ttabbed");
    }

    #[test]
    fn a_line_is_read_up_to_the_limit_and_no_further() {
        let read = |bytes: &[u8], limit| read_line(&mut BufReader::new(bytes), limit);
        // The newline counts against the limit: a line of 15 bytes fits 16, a line of 16 does not.
        assert_eq!(
            read(b"123456789012345\nrest", 16).unwrap(),
            "123456789012345"
        );
        let long = read(b"1234567890123456\nrest", 16).unwrap_err();
        assert_eq!(long.kind(), io::ErrorKind::InvalidData);
        assert!(long.to_string().contains("longer than 16 bytes"), "{long}");

        // A line that never ends is not read past the limit. A thousand bytes stand for endless, so that a reader which ignores the limit ends in the wrong error and not in memory.
        let mut endless = BufReader::new(io::repeat(b'x').take(1_000));
        let error = read_line(&mut endless, 16).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        // The line after a good one is still there.
        let mut two = BufReader::new(&b"one\ntwo\n"[..]);
        assert_eq!(read_line(&mut two, 16).unwrap(), "one");
        assert_eq!(read_line(&mut two, 16).unwrap(), "two");
    }

    #[test]
    fn a_pipe_that_closes_mid_line_is_a_process_that_left() {
        let mut half = BufReader::new(&b"{\"type\":\"rea"[..]);
        let error = read_line(&mut half, 1024).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(matches!(from_io(error), Failure::Exited));
        let mut nothing = BufReader::new(&b""[..]);
        assert_eq!(
            read_line(&mut nothing, 1024).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn a_line_of_hundreds_of_megabytes_is_one_short_log_line() {
        let endless = BufReader::new(io::repeat(b'x').take(200 * 1024 * 1024));
        let lines = forwarded(endless);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), LOG_LINE_LIMIT);
    }
}
