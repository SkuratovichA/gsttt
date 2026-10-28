//! One `claude -p` process: started in the checkout with the prompt on its standard input, and watched until it ends or its time is up. Its stream goes to the transcript file, and the init event in it is handed over as soon as it is written, so that a run that loaded the wrong things is stopped before it spends anything.

use std::fs::{self, File};
use std::io::{ErrorKind, Write as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, ensure};
use serde_json::Value;

use super::init::{self, Init};
use super::process::{POLL, kill_group};
use super::setups::Invocation;

/// What a process is started with.
#[derive(Debug)]
pub(super) struct Launch<'a> {
    pub(super) claude: &'a Path,
    pub(super) invocation: &'a Invocation,
    /// The checkout the run is made in.
    pub(super) dir: &'a Path,
    pub(super) prompt: &'a str,
    /// Where the stream goes; the standard error goes beside it, with the extension `stderr`.
    pub(super) transcript: &'a Path,
    /// The wall-clock limit.
    pub(super) limit: Duration,
}

/// How a process ended.
#[derive(Debug)]
pub(super) struct Finished {
    /// The task stopped it at the limit.
    pub(super) timed_out: bool,
    /// How long it ran, by the clock of the task.
    pub(super) elapsed: Duration,
    /// How it exited; `None` when the task stopped it.
    pub(super) exit: Option<ExitStatus>,
}

/// The file the standard error of a run goes to.
pub(super) fn stderr_of(transcript: &Path) -> PathBuf {
    transcript.with_extension("stderr")
}

/// The last `chars` characters of the standard error of a run, for the message of a task that failed.
pub(super) fn stderr_tail(transcript: &Path, chars: usize) -> String {
    let text = fs::read_to_string(stderr_of(transcript)).unwrap_or_default();
    let tail: Vec<char> = text.trim().chars().rev().take(chars).collect();
    tail.into_iter().rev().collect()
}

/// Runs the process of `launch`. `on_init` gets the init event once, and an error from it stops the process and is returned.
pub(super) fn execute(
    launch: &Launch,
    on_init: &mut dyn FnMut(&Init) -> anyhow::Result<()>,
) -> anyhow::Result<Finished> {
    let transcript = File::create(launch.transcript)
        .with_context(|| format!("cannot create {}", launch.transcript.display()))?;
    let stderr = stderr_of(launch.transcript);
    let stderr =
        File::create(&stderr).with_context(|| format!("cannot create {}", stderr.display()))?;
    let mut child = Command::new(launch.claude)
        .args(&launch.invocation.args)
        .env_clear()
        .envs(launch.invocation.env.iter().map(|(name, value)| (name, value)))
        .current_dir(launch.dir)
        .stdin(Stdio::piped())
        .stdout(transcript)
        .stderr(stderr)
        // Its own group, so that the language server and the MCP servers it starts can be stopped with it.
        .process_group(0)
        .spawn()
        .with_context(|| format!("cannot start {}", launch.claude.display()))?;
    let finished = supervise(&mut child, launch, on_init);
    // Whatever happened, nothing the run started stays alive.
    let cleaned = kill_group(&mut child);
    let finished = finished?;
    cleaned?;
    Ok(finished)
}

fn supervise(
    child: &mut Child,
    launch: &Launch,
    on_init: &mut dyn FnMut(&Init) -> anyhow::Result<()>,
) -> anyhow::Result<Finished> {
    feed(child.stdin.take(), launch.prompt)?;
    let started = Instant::now();
    let mut seen = false;
    loop {
        if !seen {
            seen = watch(launch.transcript, on_init)?;
        }
        if let Some(exit) = child.try_wait()? {
            let elapsed = started.elapsed();
            // It may have written its stream and ended between two looks.
            if !seen {
                seen = watch(launch.transcript, on_init)?;
            }
            ensure!(
                seen,
                "the stream in {} holds no init event: {}",
                launch.transcript.display(),
                stderr_tail(launch.transcript, 400)
            );
            return Ok(Finished {
                timed_out: false,
                elapsed,
                exit: Some(exit),
            });
        }
        if started.elapsed() >= launch.limit {
            kill_group(child)?;
            return Ok(Finished {
                timed_out: true,
                elapsed: started.elapsed(),
                exit: None,
            });
        }
        std::thread::sleep(POLL);
    }
}

/// Writes the prompt and closes the standard input, which is how `claude -p` knows the prompt is complete. A process that ended without reading it is not an error here: how it ended is.
fn feed(stdin: Option<ChildStdin>, prompt: &str) -> anyhow::Result<()> {
    let mut stdin = stdin.context("the process has no standard input")?;
    match stdin.write_all(prompt.as_bytes()) {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => {
            Err(error).context("cannot write the prompt")
        }
        _ => Ok(()),
    }
}

/// Looks for the init event among the lines written so far and hands it to `on_init`. A plugin's hooks may write events before it, so it is not the first line.
fn watch(
    transcript: &Path,
    on_init: &mut dyn FnMut(&Init) -> anyhow::Result<()>,
) -> anyhow::Result<bool> {
    // The file is being written: a half line, or half of a character, is read again at the next look.
    let Ok(text) = fs::read_to_string(transcript) else {
        return Ok(false);
    };
    let complete = text.rsplit_once('\n').map_or("", |(lines, _)| lines);
    for line in complete.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(found) = init::from_event(&event)? {
            on_init(&found)?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// A script that stands in for `claude` in tests: it is called with the arguments of a run, and `body` is a shell script.
#[cfg(test)]
pub(super) fn fake_claude(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let script = dir.join("claude");
    fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: &str = r#"{"type":"system","subtype":"init","tools":["Bash"],"mcp_servers":[],"plugins":[],"skills":[],"slash_commands":[],"agents":[],"model":"claude-test-1","claude_code_version":"9.9.9"}"#;

    fn launch<'a>(
        claude: &'a Path,
        invocation: &'a Invocation,
        dir: &'a Path,
        transcript: &'a Path,
        limit: Duration,
    ) -> Launch<'a> {
        Launch {
            claude,
            invocation,
            dir,
            prompt: "the question",
            transcript,
            limit,
        }
    }

    fn invocation() -> Invocation {
        Invocation {
            args: vec!["-p".into()],
            env: vec![("PATH".to_owned(), "/usr/bin:/bin".into())],
        }
    }

    #[test]
    fn a_process_is_given_the_prompt_and_nothing_of_the_environment_but_what_the_invocation_says() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(
            dir.path(),
            &format!(
                "cat > prompt.txt\nenv | sort > env.txt\npwd > cwd.txt\necho '{INIT}'\necho '{{\"type\":\"result\"}}'"
            ),
        );
        let transcript = dir.path().join("run.jsonl");
        let mut models = Vec::new();
        let finished = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(60),
            ),
            &mut |init| {
                models.push(init.model.clone());
                Ok(())
            },
        )
        .unwrap();
        assert!(!finished.timed_out);
        assert!(finished.exit.unwrap().success());
        assert_eq!(models, ["claude-test-1"]);
        assert_eq!(
            fs::read_to_string(dir.path().join("prompt.txt")).unwrap(),
            "the question"
        );
        let env = fs::read_to_string(dir.path().join("env.txt")).unwrap();
        assert!(env.contains("PATH=/usr/bin:/bin"), "{env}");
        assert!(!env.contains("HOME="), "{env}");
        assert_eq!(
            fs::canonicalize(
                fs::read_to_string(dir.path().join("cwd.txt"))
                    .unwrap()
                    .trim()
            )
            .unwrap(),
            fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[test]
    fn a_process_that_runs_past_the_limit_is_stopped_with_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(
            dir.path(),
            &format!("echo '{INIT}'\n(sleep 60; touch survivor) &\nsleep 60"),
        );
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let finished = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                // Long enough for the stand-in to start and write its init event on a machine that is busy with a build.
                Duration::from_secs(5),
            ),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(finished.timed_out && finished.exit.is_none());
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(fs::read_to_string(&transcript).unwrap().contains("init"));
    }

    #[test]
    fn a_run_that_loaded_the_wrong_things_is_stopped_at_its_init() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(dir.path(), &format!("echo '{INIT}'\nsleep 3\ntouch spent"));
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let error = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(60),
            ),
            &mut |_| anyhow::bail!("it loaded the wrong things"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("wrong things"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_secs(4));
        assert!(!dir.path().join("spent").exists(), "the process went on");
    }

    #[test]
    fn a_stream_without_an_init_event_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(dir.path(), "echo 'not logged in' >&2\nexit 1");
        let transcript = dir.path().join("run.jsonl");
        let error = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(60),
            ),
            &mut |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("not logged in"), "{error}");
    }
}
