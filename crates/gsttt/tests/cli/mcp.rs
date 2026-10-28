//! `gsttt mcp`: the MCP server as the binary runs it, spoken to over its standard streams in lines of JSON.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use gsttt_config::CONFIG_PATH;
use gsttt_scenario::scenario;
use serde_json::{Value, json};

use super::support::{Sandbox, is_empty, write};

/// How long a test waits for the server to say or do something it should do at once, before it calls the server
/// stuck. Every read is bounded by it, so that a hung server fails the test and does not hang the run.
const PATIENCE: Duration = Duration::from_secs(10);

/// The newest protocol version that still opens a session with `initialize`.
const PROTOCOL: &str = "2025-11-25";

/// What a tool call came back with when it came back as a result and not as a protocol error.
#[derive(Debug)]
struct Reply {
    /// Whether the result has `isError` set.
    is_error: bool,
    /// The text of the one block the result holds.
    text: String,
}

/// How a run of `gsttt mcp` ended.
#[derive(Debug)]
struct Ended {
    /// The exit code, `None` when a signal ended the process.
    code: Option<i32>,
    /// Every line the process wrote to stdout.
    stdout: Vec<String>,
    stderr: String,
}

/// A running `gsttt mcp`, spoken to in lines of JSON.
struct Server {
    child: Child,
    /// The child's stdin, dropped to close it.
    stdin: Option<ChildStdin>,
    /// The lines the child wrote to stdout, read on a thread so that a server that says nothing can be timed out.
    lines: Receiver<String>,
    /// Every line of stdout read so far.
    seen: Vec<String>,
    /// Collects stderr until it ends, so that a full pipe never stops the child.
    stderr: Option<JoinHandle<String>>,
    /// The id of the next request.
    next_id: u64,
}

impl Server {
    /// Starts `command`, which is `gsttt mcp` with its environment set, with every standard stream piped.
    fn start(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let stderr = thread::spawn(move || {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut stderr, &mut text).unwrap();
            text
        });
        Self {
            child,
            stdin,
            lines,
            seen: Vec::new(),
            stderr: Some(stderr),
            next_id: 1,
        }
    }

    /// Writes one message to the server.
    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("the input is still open");
        writeln!(stdin, "{message}")
            .and_then(|()| stdin.flush())
            .unwrap_or_else(|error| panic!("cannot write {message} to the server: {error}"));
    }

    /// The next line the server wrote, which a server that is stuck fails the test by not writing.
    fn line(&mut self) -> String {
        match self.lines.recv_timeout(PATIENCE) {
            Ok(line) => {
                self.seen.push(line.clone());
                line
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("the server wrote no line within {PATIENCE:?}")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("the server ended before it answered"),
        }
    }

    /// Sends a request and returns the response to it, which is the next line the server writes.
    fn request(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let line = self.line();
        let response: Value = serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("`{line}` is not JSON: {error}"));
        assert_eq!(response["id"], json!(id), "{response}");
        response
    }

    /// Opens the session: `initialize`, which must be answered with a result, then `notifications/initialized`.
    fn hello(&mut self) {
        let response = self.request(
            "initialize",
            &json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "a test", "version": "0"},
            }),
        );
        assert!(
            response.get("result").is_some(),
            "initialize succeeded: {response}"
        );
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }

    /// Checks that the server answers `tools/list`, which shows that a call that failed did not end the session.
    fn still_lists_tools(&mut self) {
        let listed = self.request("tools/list", &json!({}));
        assert!(listed["result"]["tools"].is_array(), "{listed}");
    }

    /// Calls `tool` with `arguments` and returns its result, which fails the test when the server answered with a
    /// protocol error instead.
    fn call(&mut self, tool: &str, arguments: &Value) -> Reply {
        let response = self.request("tools/call", &json!({"name": tool, "arguments": arguments}));
        let result = response
            .get("result")
            .unwrap_or_else(|| panic!("`{tool}` was answered with a protocol error: {response}"));
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("the result holds no text: {result}"));
        Reply {
            is_error: result["isError"] == json!(true),
            text: text.to_owned(),
        }
    }

    /// Calls `tool` with `arguments` and returns the text of its result, which fails the test unless the result has
    /// `isError` set.
    fn fail(&mut self, tool: &str, arguments: &Value) -> String {
        let reply = self.call(tool, arguments);
        assert!(reply.is_error, "{reply:?}");
        reply.text
    }

    /// Closes the server's input, waits for it to end, and returns what it wrote.
    fn finish(&mut self) -> Ended {
        drop(self.stdin.take());
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < PATIENCE,
                "the server did not end within {PATIENCE:?} of the end of its input"
            );
            // A process has no channel to wait on with a limit, so its end is polled.
            thread::sleep(Duration::from_millis(10));
        };
        while let Ok(line) = self.lines.recv_timeout(PATIENCE) {
            self.seen.push(line);
        }
        Ended {
            code: status.code(),
            stdout: std::mem::take(&mut self.seen),
            stderr: self.stderr.take().unwrap().join().unwrap(),
        }
    }
}

impl Drop for Server {
    /// A test that fails leaves no server behind.
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

/// `gsttt mcp`, started in `dir`.
fn mcp_in(sandbox: &Sandbox, dir: &Path) -> Command {
    let mut command = sandbox.gsttt();
    command.current_dir(dir).arg("mcp");
    command
}

#[scenario("mcp", "gsttt mcp answers about its repository")]
#[test]
fn gsttt_mcp_answers_about_its_repository() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let mut server = Server::start(mcp_in(&sandbox, &repo));
    server.hello();

    let reply = server.call("dependents", &json!({"path": "lib/util.ts"}));

    assert!(!reply.is_error, "{reply:?}");
    assert!(reply.text.contains("app/a.ts"), "{}", reply.text);
    // The text is the one the command line prints, which ends its last line with a newline the result does not carry.
    let printed = sandbox.run_in(&repo, &["dependents", "lib/util.ts"]);
    assert_eq!(printed.code, Some(0), "{printed:?}");
    assert_eq!(format!("{}\n", reply.text), printed.stdout);
    assert_eq!(server.finish().code, Some(0));
}

#[scenario("mcp", "A refused root fails the call, not the server")]
#[test]
fn a_refused_root_fails_the_call_not_the_server() {
    let sandbox = Sandbox::new();
    let mut server = Server::start(mcp_in(&sandbox, sandbox.home.path()));
    server.hello();

    let reply = server.call("coverage", &json!({}));

    assert!(reply.is_error, "{reply:?}");
    assert!(
        reply.text.contains("does not index the home directory"),
        "{}",
        reply.text
    );
    // The server is still there: it answers one more request.
    server.still_lists_tools();
    let ended = server.finish();
    assert_eq!(ended.code, Some(0), "{ended:?}");
    for written in [&sandbox.home, &sandbox.cache, &sandbox.state] {
        assert!(is_empty(written.path()), "{:?} was written", written.path());
    }
}

#[scenario(
    "mcp",
    "A failure the command line gives a help line for ends in a step for the model"
)]
#[test]
fn a_failure_the_command_line_has_a_help_line_for_ends_in_a_step_for_the_model() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let plain = sandbox.work.path().join("plain");
    std::fs::create_dir(&plain).unwrap();

    // A root that the server refuses and cannot leave, so the step asks the person.
    let mut server = Server::start(mcp_in(&sandbox, sandbox.home.path()));
    server.hello();
    let refused = server.fail("coverage", &json!({}));
    assert!(
        refused.contains("does not index the home directory"),
        "{refused}"
    );
    assert!(
        refused.ends_with("or to name one with `-C <dir>`"),
        "{refused}"
    );
    assert_eq!(server.finish().code, Some(0));

    // Questions that a model can ask differently.
    let mut server = Server::start(mcp_in(&sandbox, &repo));
    server.hello();
    let outside = server.fail("dependents", &json!({"path": "../elsewhere.ts"}));
    assert!(outside.contains("is outside the repository"), "{outside}");
    assert!(outside.ends_with("for example `src/a.ts`"), "{outside}");
    let base = server.fail(
        "impact",
        &json!({"changed": true, "base": "no-such-branch"}),
    );
    assert!(
        base.contains("`no-such-branch` is not a revision"),
        "{base}"
    );
    assert!(
        base.ends_with("or leave `base` out to compare with HEAD"),
        "{base}"
    );
    assert_eq!(server.finish().code, Some(0));

    // A directory outside git has no history to take the changed files from.
    let mut server = Server::start(mcp_in(&sandbox, &plain));
    server.hello();
    let no_git = server.fail("impact", &json!({"changed": true}));
    assert!(no_git.contains("needs a git repository"), "{no_git}");
    assert!(no_git.ends_with("and leave `changed` out"), "{no_git}");
    assert_eq!(server.finish().code, Some(0));
}

/// Serves a repository whose config is `broken` and calls `coverage`, which fails the call and not the server: `initialize` has succeeded before it, `tools/list` still answers after it, and the next call reads the config again, so that it answers once the config is `fixed`. Returns the failed call.
fn fails_until_fixed(broken: &str, fixed: &str) -> Reply {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, CONFIG_PATH, broken);
    let mut server = Server::start(mcp_in(&sandbox, &repo));
    server.hello();

    let failed = server.call("coverage", &json!({}));

    assert!(failed.is_error, "{failed:?}");
    assert!(
        failed.text.contains("; fix the config and ask again"),
        "{}",
        failed.text
    );
    server.still_lists_tools();
    write(&repo, CONFIG_PATH, fixed);
    let answered = server.call("coverage", &json!({}));
    assert!(!answered.is_error, "{answered:?}");
    assert!(
        answered.text.contains("ts read 3 of 3 files"),
        "{}",
        answered.text
    );
    let ended = server.finish();
    assert_eq!(ended.code, Some(0), "{ended:?}");
    failed
}

#[scenario("mcp", "A config that does not read fails the call, not the server")]
#[test]
fn a_config_that_does_not_read_fails_the_call_not_the_server() {
    let failed = fails_until_fixed("detect = true\n\n[ts\n", "detect = true\n\n[ts]\n");

    assert!(
        failed.text.contains(".gsttt/config.toml:3:"),
        "{}",
        failed.text
    );
}

#[scenario("mcp", "Options a module cannot read fail the call, not the server")]
#[test]
fn options_a_module_cannot_read_fail_the_call_not_the_server() {
    let failed = fails_until_fixed("[ts]\nno_such_option = true\n", "[ts]\n");

    assert!(failed.text.contains("`ts`"), "{}", failed.text);
    assert!(failed.text.contains("no_such_option"), "{}", failed.text);
}

#[scenario("mcp", "stdout carries only the protocol")]
#[test]
fn stdout_carries_only_the_protocol() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let mut command = mcp_in(&sandbox, &repo);
    command.env("GSTTT_LOG", "debug");
    let mut server = Server::start(command);
    server.hello();
    let listed = server.request("tools/list", &json!({}));
    assert!(listed["result"]["tools"].is_array(), "{listed}");
    let reply = server.call("coverage", &json!({}));
    assert!(!reply.is_error, "{reply:?}");

    let ended = server.finish();

    assert_eq!(ended.code, Some(0), "{ended:?}");
    // The answers to the three requests are there, so the check below is not about an empty stdout.
    assert!(ended.stdout.len() >= 3, "{:?}", ended.stdout);
    for line in &ended.stdout {
        let message: Value = serde_json::from_str(line)
            .unwrap_or_else(|error| panic!("`{line}` on stdout is not JSON: {error}"));
        assert_eq!(message["jsonrpc"], json!("2.0"), "{line}");
    }
    // The logs were on, and they went to stderr.
    assert!(ended.stderr.contains("DEBUG"), "{}", ended.stderr);
}

#[scenario("mcp", "gsttt mcp refuses the flags of the command line")]
#[test]
fn gsttt_mcp_refuses_the_flags_of_the_command_line() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    for flag in ["--json", "--all", "--no-refresh"] {
        let ran = sandbox.run_in(&repo, &["mcp", flag]);
        assert_eq!(ran.code, Some(2), "{flag}: {ran:?}");
        assert_eq!(ran.stdout, "", "{flag}");
        assert!(ran.stderr.contains(flag), "{flag}: {}", ran.stderr);
        assert!(ran.stderr.contains("takes none"), "{flag}: {}", ran.stderr);
    }
}

#[scenario("mcp", "gsttt mcp exits 0 when its input closes")]
#[test]
fn gsttt_mcp_exits_0_when_its_input_closes() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let mut server = Server::start(mcp_in(&sandbox, &repo));
    server.hello();

    let ended = server.finish();

    assert_eq!(ended.code, Some(0), "{ended:?}");
}

#[scenario("mcp", "An untrusted external is not started by the server")]
#[test]
fn an_untrusted_external_is_not_started_by_the_server() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    // Checked once more when the server is gone, because a connector that a later call or the shutdown started would leave its marks after the first check.
    let nothing_ran = |by: &str| {
        assert!(
            !fake.marker.exists(),
            "the server started the connector by {by}"
        );
        assert!(
            is_empty(sandbox.state.path()),
            "the trust store was written by {by}"
        );
    };
    let mut server = Server::start(mcp_in(&sandbox, &fake.root));
    server.hello();

    let reply = server.call("dependents", &json!({"path": "lib/util.ts"}));

    assert!(!reply.is_error, "{reply:?}");
    assert!(reply.text.contains("app/a.ts"), "{}", reply.text);
    nothing_ran("the first answer");
    // The server goes on answering, with the files of the connector among those no connector read, and it never asked.
    let coverage = server.call("coverage", &json!({}));
    assert!(!coverage.is_error, "{coverage:?}");
    assert!(coverage.text.contains(".fake  1 file"), "{}", coverage.text);
    let ended = server.finish();
    assert_eq!(ended.code, Some(0), "{ended:?}");
    assert!(!ended.stderr.contains("[y/N]"), "{}", ended.stderr);
    nothing_ran("the end of the session");
}

/// The control of the scenario above. A marker that never appears says nothing when the connector cannot start at all, so the same repository is trusted at a terminal while the server runs, and the next call starts the connector.
#[test]
fn trust_given_at_a_terminal_reaches_a_running_server() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    let mut server = Server::start(mcp_in(&sandbox, &fake.root));
    server.hello();
    let before = server.call("coverage", &json!({}));
    assert!(!before.is_error, "{before:?}");
    assert!(!fake.marker.exists(), "the server started the connector");

    let granted = sandbox.run_typing(&fake.root, &["config", "trust"], "y\n");
    assert_eq!(granted.code, Some(0), "{granted:?}");
    let after = server.call("coverage", &json!({}));

    assert!(!after.is_error, "{after:?}");
    assert!(
        fake.marker.exists(),
        "the trusted connector was not started"
    );
    assert_eq!(server.finish().code, Some(0));
}
