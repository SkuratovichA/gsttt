//! One `claude -p` process: started in the checkout with the prompt on its standard input, and watched until it ends or its time is up. Its stream goes to the transcript file, and the init event in it is handed over as soon as it is written, so that a run that loaded the wrong things is stopped as soon as the event is written, which is after its first request is sent. The result event ends the run: `claude -p` stays alive after it for as long as a task the model left in the background runs, and that time is no part of the answer.

use std::fs::{self, File};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, ensure};
use serde_json::Value;

use super::init::{self, Init};
use super::process::{self, Finished, Job, Look, Stop};
use super::setups::Invocation;

/// What a process is started with.
#[derive(Debug)]
pub(super) struct Launch<'a> {
    pub(super) claude: &'a Path,
    pub(super) invocation: &'a Invocation,
    /// The checkout the run is made in.
    pub(super) dir: &'a Path,
    /// The temporary directory of the run, which is removed when it ends: what still works in it then is ended with the run, as what works in the checkout is.
    pub(super) tmp: &'a Path,
    pub(super) prompt: &'a str,
    /// Where the stream goes; the standard error goes beside it, with the extension `stderr`.
    pub(super) transcript: &'a Path,
    /// The wall-clock limit.
    pub(super) limit: Duration,
    /// Ctrl-C, SIGTERM and SIGHUP on the task.
    pub(super) stop: &'a Stop,
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

/// Runs the process of `launch`. `on_init` gets the init event once, and an error from it stops the process and is returned; so is a process that ended in any way without having written one. The process is ended when its result is written and it has not exited five seconds later, and the checkout and the temporary directory of the run are the directories whose other processes are ended with it.
pub(super) fn execute(
    launch: &Launch,
    on_init: &mut dyn FnMut(&Init) -> anyhow::Result<()>,
) -> anyhow::Result<Finished> {
    let transcript = File::create(launch.transcript)
        .with_context(|| format!("cannot create {}", launch.transcript.display()))?;
    let stderr = stderr_of(launch.transcript);
    let stderr =
        File::create(&stderr).with_context(|| format!("cannot create {}", stderr.display()))?;
    let mut command = Command::new(launch.claude);
    command
        .args(&launch.invocation.args)
        .env_clear()
        .envs(
            launch
                .invocation
                .env
                .iter()
                .map(|(name, value)| (name, value)),
        )
        .current_dir(launch.dir)
        .stdout(transcript)
        .stderr(stderr);
    let mut stream = Stream::new(launch.transcript);
    let finished = process::run(
        Job {
            command,
            input: Some(launch.prompt),
            dirs: &[launch.dir, launch.tmp],
            limit: launch.limit,
            stop: launch.stop,
        },
        &mut || stream.look(on_init),
    )?;
    // A run that never wrote its init event loaded nothing that was checked, so whatever became of its process, a timeout of it says nothing about its setup.
    ensure!(
        stream.init_seen,
        "the stream in {} holds no init event after {:?}: {}",
        launch.transcript.display(),
        finished.elapsed,
        stderr_tail(launch.transcript, 400)
    );
    Ok(finished)
}

/// The stream file of a run, read as it grows: only the bytes written since the last look are read.
#[derive(Debug)]
struct Stream<'a> {
    transcript: &'a Path,
    /// How many bytes are handled: the lines up to the last line break.
    offset: u64,
    init_seen: bool,
}

impl<'a> Stream<'a> {
    fn new(transcript: &'a Path) -> Self {
        Self {
            transcript,
            offset: 0,
            init_seen: false,
        }
    }

    /// Handles the lines written since the last look: the init event goes to `on_init`, and a result event says the run has nothing more to say. A plugin's hooks may write events before the init, so it is not the first line.
    fn look(
        &mut self,
        on_init: &mut dyn FnMut(&Init) -> anyhow::Result<()>,
    ) -> anyhow::Result<Look> {
        let Ok(mut file) = File::open(self.transcript) else {
            return Ok(Look::Working);
        };
        file.seek(SeekFrom::Start(self.offset))?;
        let mut fresh = Vec::new();
        file.read_to_end(&mut fresh)?;
        // The file is being written: a half line, or half of a character, is read again at the next look.
        let Some((complete, _)) = fresh
            .iter()
            .rposition(|&byte| byte == b'\n')
            .and_then(|end| fresh.split_at_checked(end + 1))
        else {
            return Ok(Look::Working);
        };
        self.offset += u64::try_from(complete.len())?;
        let mut look = Look::Working;
        for line in String::from_utf8_lossy(complete).lines() {
            let Ok(event) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if !self.init_seen
                && let Some(found) = init::from_event(&event)?
            {
                on_init(&found)?;
                self.init_seen = true;
            }
            if event.get("type").and_then(Value::as_str) == Some("result") {
                look = Look::Done;
            }
        }
        Ok(look)
    }
}

/// A script that stands in for `claude` in tests: it is called with the arguments of a run, and `body` is a shell script.
#[cfg(test)]
pub(super) fn fake_claude(dir: &Path, body: &str) -> PathBuf {
    fake_program(dir, "claude", body)
}

/// A script named `name` in `dir` that stands in for a program in tests, `body` being a shell script.
#[cfg(test)]
pub(super) fn fake_program(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let script = dir.join(name);
    fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;
    use std::time::Instant;

    use super::super::process::GRACE;
    use super::super::process::stand_in::Grandchild;
    use super::*;

    const INIT: &str = r#"{"type":"system","subtype":"init","tools":["Bash"],"mcp_servers":[],"plugins":[],"skills":[],"slash_commands":[],"agents":[],"model":"claude-test-1","claude_code_version":"9.9.9","session_id":"s-1"}"#;

    const RESULT: &str =
        r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#;

    /// A stop that nothing raises.
    static NEVER: LazyLock<Stop> = LazyLock::new(Stop::default);

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
            tmp: dir,
            prompt: "the question",
            transcript,
            limit,
            stop: &NEVER,
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
        // The background job stands in `/`, outside the checkout, so that only the process group can reach it, and the stand-in writes its id for the test to look at once the run is over.
        let claude = fake_claude(
            dir.path(),
            &format!("echo '{INIT}'\n(cd / && sleep 60) &\necho $! > pid\nsleep 60"),
        );
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let finished = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                // Long enough for the stand-in to start and write its init event and its id on a machine that is busy with a build.
                Duration::from_secs(3),
            ),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(finished.timed_out && finished.exit.is_none());
        // Ten times the limit, with the stand-in, which stays on for a minute, thirty seconds more to end by itself.
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(fs::read_to_string(&transcript).unwrap().contains("init"));
        assert!(
            Grandchild::of(dir.path()).is_gone_soon(),
            "what the process started went on"
        );
    }

    #[test]
    fn a_process_that_stays_alive_after_its_result_is_recorded_from_it_and_not_as_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(
            dir.path(),
            &format!("echo '{INIT}'\necho '{RESULT}'\nsleep 60"),
        );
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let finished = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(40),
            ),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(!finished.timed_out, "the answer was written");
        assert!(finished.exit.is_none(), "the task ended it");
        // Its time stops at the result, and it was ended after the grace, before the limit. The upper bound is six times the grace and leaves the stand-in, which stays on for a minute, thirty seconds more to end by itself.
        assert!(finished.elapsed < GRACE, "{:?}", finished.elapsed);
        assert!(started.elapsed() >= GRACE && started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn a_run_that_loaded_the_wrong_things_is_stopped_at_its_init() {
        let dir = tempfile::tempdir().unwrap();
        // It would touch the file at 3 s, and end at 33 s: a task that left it on would not look as if it had ended it.
        let claude = fake_claude(
            dir.path(),
            &format!("echo '{INIT}'\nsleep 3\ntouch spent\nsleep 30"),
        );
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
        assert!(started.elapsed() < Duration::from_secs(15));
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

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A run stopped at its limit with no init event is an error"
    )]
    #[test]
    fn a_process_that_runs_past_the_limit_without_an_init_event_is_an_error_and_not_a_timeout() {
        let dir = tempfile::tempdir().unwrap();
        // It waits, as `claude` does for a login or a hook, and has written nothing when its time is up: nothing it loaded was checked.
        let claude = fake_claude(dir.path(), "echo 'waiting for the login' >&2\nsleep 60");
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let error = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(3),
            ),
            &mut |_| Ok(()),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("run.jsonl") && error.contains("holds no init event after "),
            "{error}"
        );
        assert!(error.contains("waiting for the login"), "{error}");
        // The limit ended the stand-in, which stays on for a minute.
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn a_process_that_stays_alive_after_a_result_without_an_init_event_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let claude = fake_claude(
            dir.path(),
            &format!("echo 'no init, only an answer' >&2\necho '{RESULT}'\nsleep 60"),
        );
        let transcript = dir.path().join("run.jsonl");
        let started = Instant::now();
        let error = execute(
            &launch(
                &claude,
                &invocation(),
                dir.path(),
                &transcript,
                Duration::from_secs(40),
            ),
            &mut |_| Ok(()),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("holds no init event") && error.contains("no init, only an answer"),
            "{error}"
        );
        // The task ended it after the grace, long before its limit and before it ended by itself.
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn a_stream_is_read_once_whatever_the_size_of_the_pieces_it_is_written_in() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("run.jsonl");
        let add = |bytes: &[u8]| {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&transcript)
                .unwrap();
            std::io::Write::write_all(&mut file, bytes).unwrap();
        };
        let inits = std::cell::Cell::new(0);
        let look = |stream: &mut Stream| {
            stream
                .look(&mut |_| {
                    inits.set(inits.get() + 1);
                    Ok(())
                })
                .unwrap()
        };
        let mut stream = Stream::new(&transcript);
        assert_eq!(look(&mut stream), Look::Working, "no file yet");

        // The init event comes in two pieces, the second with its line break. The result event comes after it, with a character of two bytes in its text, and the character is cut between two pieces.
        let (init, rest) = INIT.split_at(40);
        let (first, second) = "\u{e9}".as_bytes().split_at(1);
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":""#;
        add(init.as_bytes());
        assert_eq!(look(&mut stream), Look::Working);
        assert_eq!(inits.get(), 0, "half a line is not handed over");
        add(&[rest.as_bytes(), b"\n", result.as_bytes(), first].concat());
        assert_eq!(look(&mut stream), Look::Working);
        assert!(stream.init_seen);
        assert_eq!(inits.get(), 1);
        add(&[second, b"\"}\n"].concat());
        assert_eq!(look(&mut stream), Look::Done, "the cut line was read whole");
        // A second init event, in a later piece, is not handed over again.
        add(format!("{INIT}\n").as_bytes());
        assert_eq!(look(&mut stream), Look::Working);
        assert_eq!(inits.get(), 1, "the init event was handed over twice");

        // The whole file read in one piece gives the same: one init event handed over, and the result found.
        let mut once = 0;
        let found = Stream::new(&transcript)
            .look(&mut |_| {
                once += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!((found, once), (Look::Done, 1));
    }
}
