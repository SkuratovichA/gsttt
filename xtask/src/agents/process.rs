//! Running programs for the agent benchmarks: finding one on a PATH, and waiting for one that runs in a process group of its own, which is ended together with everything it started.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail, ensure};
use rustix::io::Errno;
use rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process, kill_process_group, waitid,
};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::{flag, low_level};
use xshell::Shell;

use super::checkout::{self, with_canonical_names};

/// How often a running process is looked at while it is waited for.
pub(super) const POLL: Duration = Duration::from_millis(50);

/// How long a process whose work is done has to exit before it is ended.
pub(super) const GRACE: Duration = Duration::from_secs(5);

/// The first executable file called `program` in the directories of `path`.
pub(super) fn which(program: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(file: &Path) -> bool {
    fs::metadata(file).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Ctrl-C and `kill` on the task. A process the task waits for is in a group of its own, so the signal reaches the task alone and leaves that process running. Once the signals are caught, the wait ends the process and stops the task with an error instead; a second signal ends the task on the spot, for the wait that does not end.
#[derive(Clone, Debug, Default)]
pub(super) struct Stop {
    /// The number of the signal that arrived, or 0.
    signal: Arc<AtomicUsize>,
    /// Set by the first signal, and read by the handler that runs the default action of the second.
    arrived: Arc<AtomicBool>,
}

/// What the signals of this process are caught into. A handler cannot be taken back, so the handlers are registered once, whatever the number of calls to [`Stop::catch`].
static CAUGHT: Mutex<Option<Stop>> = Mutex::new(None);

impl Stop {
    /// Catches SIGINT and SIGTERM for the rest of the task, in place of ending it on the spot. The first call registers the handlers and every call returns the same `Stop`. A signal that comes between two waits is seen by the next, and a second signal ends the task at once.
    pub(super) fn catch() -> anyhow::Result<Self> {
        let mut caught = CAUGHT.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(stop) = caught.as_ref() {
            return Ok(stop.clone());
        }
        let stop = Self::default();
        for signal in [SIGINT, SIGTERM] {
            // The handlers run in the order they are registered: the first signal finds `arrived` unset, and sets it after the default action has declined to run, and the second finds it set.
            let number = usize::try_from(signal)?;
            flag::register_conditional_default(signal, Arc::clone(&stop.arrived))
                .and_then(|_| flag::register(signal, Arc::clone(&stop.arrived)))
                .and_then(|_| flag::register_usize(signal, Arc::clone(&stop.signal), number))
                .with_context(|| format!("cannot catch signal {signal}"))?;
        }
        *caught = Some(stop.clone());
        Ok(stop)
    }

    /// Fails, naming the signal, when one has arrived.
    fn check(&self) -> anyhow::Result<()> {
        let number = self.signal.load(Ordering::Relaxed);
        if number == 0 {
            return Ok(());
        }
        let name = i32::try_from(number)
            .ok()
            .and_then(low_level::signal_name)
            .unwrap_or("a signal");
        bail!(
            "the task was stopped by {name}; the process it waited for and what that started are ended, and the run is not recorded"
        )
    }

    /// Makes `signal` arrive, for the tests that stop a wait.
    #[cfg(test)]
    pub(super) fn raise(&self, signal: i32) {
        self.signal
            .store(usize::try_from(signal).unwrap(), Ordering::Relaxed);
    }
}

/// What a look at a running process found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Look {
    Working,
    /// It has done what it was run for and has only to exit: it gets [`GRACE`] to, and is ended after that.
    Done,
}

/// A program to run in a process group of its own and to wait for.
#[derive(Debug)]
pub(super) struct Job<'a> {
    /// The program with its arguments, environment, directory and output. The group is set here, and the standard input is piped when there is an input.
    pub(super) command: Command,
    /// Written to the standard input, which is then closed.
    pub(super) input: Option<&'a str>,
    /// The directories whose processes are ended with the group: the Bash tool of Claude Code starts every command in a session of its own, which a signal to the group does not reach, but it leaves the working directory of the run. Each is below a directory the task made itself, or the job is refused before it starts.
    pub(super) dirs: &'a [&'a Path],
    /// The wall-clock limit.
    pub(super) limit: Duration,
    pub(super) stop: &'a Stop,
}

/// How a process ended.
#[derive(Debug)]
pub(super) struct Finished {
    /// The task stopped it at the limit.
    pub(super) timed_out: bool,
    /// How long it ran, by the clock of the task: until it exited, until `look` found its work done, or until the limit.
    pub(super) elapsed: Duration,
    /// How it exited; `None` when the task ended it.
    pub(super) exit: Option<ExitStatus>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    Exited,
    Limit,
    /// Its work was done and it did not exit within the grace.
    Lingered,
}

/// Starts the job and waits for it, calling `look` at every poll until it says the work is done. Whatever happened, the group is killed afterwards with every process that has a directory of the job as its working directory, so nothing the job started stays alive, not even after a normal exit. A signal that was caught ends the job the same way and is an error.
pub(super) fn run(
    job: Job,
    look: &mut dyn FnMut() -> anyhow::Result<Look>,
) -> anyhow::Result<Finished> {
    let Job {
        mut command,
        input,
        dirs,
        limit,
        stop,
    } = job;
    stop.check()?;
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .process_group(0)
        .spawn()
        .with_context(|| format!("cannot start {}", command.get_program().to_string_lossy()))?;
    let waited = feed(&mut child, input).and_then(|()| wait(&child, limit, stop, look));
    let ended = end(&mut child, dirs);
    let (ending, elapsed) = waited?;
    let status = ended?;
    Ok(Finished {
        timed_out: ending == Ending::Limit,
        elapsed,
        exit: (ending == Ending::Exited).then_some(status),
    })
}

/// Writes the input and closes the standard input, which is how `claude -p` knows the prompt is complete. A process that ended without reading it is not an error here: how it ended is.
fn feed(child: &mut Child, input: Option<&str>) -> anyhow::Result<()> {
    let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) else {
        return Ok(());
    };
    match stdin.write_all(input.as_bytes()) {
        Err(error) if error.kind() != ErrorKind::BrokenPipe => {
            Err(error).context("cannot write the input")
        }
        _ => Ok(()),
    }
}

fn wait(
    child: &Child,
    limit: Duration,
    stop: &Stop,
    look: &mut dyn FnMut() -> anyhow::Result<Look>,
) -> anyhow::Result<(Ending, Duration)> {
    let started = Instant::now();
    // How long it had run when its work was found done, and the moment.
    let mut done: Option<(Duration, Instant)> = None;
    loop {
        stop.check()?;
        if done.is_none() && look()? == Look::Done {
            done = Some((started.elapsed(), Instant::now()));
        }
        if has_exited(child) {
            // It may have written what `look` is after and ended between two looks.
            if done.is_none() {
                look()?;
            }
            let elapsed = done.map_or_else(|| started.elapsed(), |(took, _)| took);
            return Ok((Ending::Exited, elapsed));
        }
        match done {
            Some((took, since)) if since.elapsed() >= GRACE => return Ok((Ending::Lingered, took)),
            None if started.elapsed() >= limit => return Ok((Ending::Limit, started.elapsed())),
            _ => {}
        }
        std::thread::sleep(POLL);
    }
}

/// Whether the process has exited, without collecting it: a process that is collected gives up its id, and its id is the name of the group that [`end`] signals. The same call as `has_exited` in `crates/config/src/external/process.rs`, which this crate cannot reach.
fn has_exited(child: &Child) -> bool {
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    // An error means there is nothing to wait for, which is as good as an exit.
    !matches!(
        waitid(WaitId::Pid(Pid::from_child(child)), options),
        Ok(None)
    )
}

/// Kills the process group of `child`, which was started with `process_group(0)`, collects `child`, and then kills every other process that works in one of `dirs`. A test runner, a language server or an MCP server starts processes of its own and a group reaches them, but the Bash tool of Claude Code starts its commands in sessions of their own, which leaves them and their children alive when `claude` is gone.
fn end(child: &mut Child, dirs: &[&Path]) -> anyhow::Result<ExitStatus> {
    match kill_process_group(Pid::from_child(child), Signal::KILL) {
        // A group with no process left is the usual case after a process that ended on its own. macOS says "not permitted" for a group whose members are all gone but not collected yet.
        Ok(()) | Err(Errno::SRCH | Errno::PERM) => {}
        Err(error) => return Err(error).context("cannot kill the process group"),
    }
    let status = child.wait()?;
    sweep(dirs)?;
    Ok(status)
}

/// Kills every process whose working directory is one of `dirs` or below one, except this process and its parents: the shell the task was started from may stand in a directory of the sweep.
///
/// A directory that is not absolute, or is not below a directory the task made itself ([`checkout::made_by_the_task`]), is refused before any process is listed or killed: an empty path, `/` or the home directory would otherwise end every process of the user.
fn sweep(dirs: &[&Path]) -> anyhow::Result<()> {
    sweep_below(dirs, &sweep_roots())
}

/// The directories the task may end processes in: those it made, by the name it gave them and by their canonical name. The tests make theirs in the temporary directory of the system, which is no place for the task itself.
fn sweep_roots() -> Vec<PathBuf> {
    let made = checkout::made_by_the_task();
    #[cfg(test)]
    let made = made.into_iter().chain([std::env::temp_dir()]);
    with_canonical_names(made)
}

/// [`sweep`] with the directories it may end processes in given.
fn sweep_below(dirs: &[&Path], roots: &[PathBuf]) -> anyhow::Result<()> {
    let dirs = sweepable_dirs(dirs, roots)?;
    let keep = ancestry()?;
    for (pid, directory) in working_directories()? {
        if keep.contains(&pid) || !dirs.iter().any(|dir| directory.starts_with(dir)) {
            continue;
        }
        let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) else {
            continue;
        };
        match kill_process(pid, Signal::KILL) {
            // It may have ended since it was listed.
            Ok(()) | Err(Errno::SRCH) => {}
            Err(error) => {
                return Err(error).with_context(|| format!("cannot kill process {pid:?}"));
            }
        }
    }
    Ok(())
}

/// `dirs` in the form that working directories are read in, or an error naming the first one that is not the task's to sweep. Every directory is looked at before the first process is, so one wrong directory ends nothing.
fn sweepable_dirs(dirs: &[&Path], roots: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    dirs.iter()
        .map(|&dir| {
            let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
            // The name as given must be absolute too: a relative one is read against the directory this process stands in.
            ensure!(
                dir.is_absolute() && sweepable(&canonical, roots),
                "will not end the processes working in `{}`: it is not below a directory this task made",
                dir.display()
            );
            Ok(canonical)
        })
        .collect()
}

/// Whether `dir` is absolute, has no `..` in it, and is below one of `roots` and not one of them: ending the processes of a whole root would end those of the other runs and of the draw.
fn sweepable(dir: &Path, roots: &[PathBuf]) -> bool {
    dir.is_absolute()
        && !dir.components().any(|part| part == Component::ParentDir)
        && roots
            .iter()
            .any(|root| root.is_absolute() && dir != root && dir.starts_with(root))
}

/// This process and its parents, by the table of `ps`.
fn ancestry() -> anyhow::Result<BTreeSet<u32>> {
    let table = Shell::new()?
        .cmd("ps")
        .args(["-A", "-o", "pid=,ppid="])
        .quiet()
        .ignore_stderr()
        .read()?;
    let parents: BTreeMap<u32, u32> = table
        .lines()
        .filter_map(|line| {
            let mut ids = line.split_whitespace().map(str::parse::<u32>);
            Some((ids.next()?.ok()?, ids.next()?.ok()?))
        })
        .collect();
    let mut chain = BTreeSet::new();
    let mut pid = std::process::id();
    while chain.insert(pid) {
        let Some(parent) = parents.get(&pid) else {
            break;
        };
        pid = *parent;
    }
    Ok(chain)
}

/// The working directory of every process that can be looked at. Linux says it in `/proc`; macOS has none, and `lsof` lists the working directories there.
fn working_directories() -> anyhow::Result<Vec<(u32, PathBuf)>> {
    let listed = if let Ok(processes) = fs::read_dir("/proc") {
        processes
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let pid = entry.file_name().to_str()?.parse().ok()?;
                Some((pid, fs::read_link(entry.path().join("cwd")).ok()?))
            })
            .collect()
    } else {
        let output = Shell::new()?
            .cmd("lsof")
            .args(["-a", "-d", "cwd", "-Fpn"])
            .quiet()
            .ignore_status()
            .output()
            .context("cannot list the working directories of the processes")?;
        parse_lsof(&String::from_utf8_lossy(&output.stdout))
    };
    // A listing without this process in it is a listing that cannot be trusted to hold the others.
    ensure!(
        listed.iter().any(|(pid, _)| *pid == std::process::id()),
        "the working directories of the processes were listed without this process in them"
    );
    Ok(listed)
}

/// The processes and working directories in the output of `lsof -a -d cwd -Fpn`: a line `p<pid>` begins a process, and a line `n<path>` names its directory.
fn parse_lsof(text: &str) -> Vec<(u32, PathBuf)> {
    let mut found = Vec::new();
    let mut process = None;
    for line in text.lines() {
        if let Some(pid) = line.strip_prefix('p') {
            process = pid.parse().ok();
        } else if let (Some(path), Some(pid)) = (line.strip_prefix('n'), process) {
            found.push((pid, PathBuf::from(path)));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use rustix::process::getpid;

    use super::*;

    #[test]
    fn a_program_is_found_in_the_first_directory_that_holds_it() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for dir in [&first, &second] {
            let program = dir.path().join("tool");
            fs::write(&program, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(which("tool", &path), Some(first.path().join("tool")));
        assert_eq!(which("other", &path), None);
    }

    #[test]
    fn a_file_that_is_not_executable_is_not_a_program() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tool"), "text").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(which("tool", &path), None);
    }

    /// `script` run by `sh` in `dir`, with nothing to look at.
    fn job<'a>(script: &str, dirs: &'a [&'a Path], limit: Duration, stop: &'a Stop) -> Job<'a> {
        let mut command = Command::new("sh");
        command
            .args(["-c", script])
            .current_dir(dirs[0])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Job {
            command,
            input: None,
            dirs,
            limit,
            stop,
        }
    }

    fn working(job: Job) -> anyhow::Result<Finished> {
        run(job, &mut || Ok(Look::Working))
    }

    /// A background child that stands in `/`, so that only the group can reach it, and touches `marker` after 3 s.
    fn background(marker: &Path) -> String {
        format!(
            "(cd / && sleep 3 && touch '{}') & sleep 60",
            marker.display()
        )
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The group of a run ends with it")]
    #[test]
    fn a_process_that_outlives_its_limit_is_ended_with_the_group_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("survivor");
        let stop = Stop::default();
        let finished = working(job(
            &background(&marker),
            &[dir.path()],
            Duration::from_secs(2),
            &stop,
        ))
        .unwrap();
        assert!(finished.timed_out && finished.exit.is_none());
        // Past the touch.
        std::thread::sleep(Duration::from_secs(2));
        assert!(!marker.exists(), "the background child went on");
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A process out of the group is ended by its directory"
    )]
    #[test]
    fn a_process_that_left_the_group_and_the_session_is_ended_by_its_directory() {
        let dir = tempfile::tempdir().unwrap();
        // `perl` for `setsid`, which macOS has no program for: the child leaves the group and the session, as the commands of the Bash tool of Claude Code do, and keeps the directory.
        let script = r#"perl -e 'use POSIX; POSIX::setsid(); open(my $r, ">", "ready"); close($r); sleep 3; open(my $f, ">", "survivor"); close($f)' & sleep 60"#;
        let stop = Stop::default();
        let finished = working(job(script, &[dir.path()], Duration::from_secs(2), &stop)).unwrap();
        assert!(finished.timed_out);
        assert!(
            dir.path().join("ready").exists(),
            "the child had left the group before the limit"
        );
        // Past the touch.
        std::thread::sleep(Duration::from_secs(2));
        assert!(
            !dir.path().join("survivor").exists(),
            "the child out of the group went on"
        );
    }

    #[test]
    fn the_group_is_ended_after_a_normal_exit_too() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("survivor");
        let stop = Stop::default();
        let script = format!("(cd / && sleep 2 && touch '{}') & exit 0", marker.display());
        let finished =
            working(job(&script, &[dir.path()], Duration::from_secs(60), &stop)).unwrap();
        assert!(!finished.timed_out);
        assert!(finished.exit.unwrap().success());
        std::thread::sleep(Duration::from_secs(3));
        assert!(!marker.exists(), "the child outlived its parent");
    }

    #[test]
    fn a_process_whose_work_is_done_has_the_grace_to_exit_and_is_ended_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let stop = Stop::default();
        // It exits on its own within the grace: nothing is cut short.
        let finished = run(
            job("sleep 1", &[dir.path()], Duration::from_secs(60), &stop),
            &mut || Ok(Look::Done),
        )
        .unwrap();
        assert!(finished.exit.unwrap().success() && !finished.timed_out);
        assert!(finished.elapsed < Duration::from_secs(1));

        // It stays on, past the limit: it is ended after the grace, and the limit is no timeout.
        let started = Instant::now();
        let finished = run(
            job("sleep 60", &[dir.path()], Duration::from_secs(2), &stop),
            &mut || Ok(Look::Done),
        )
        .unwrap();
        assert!(!finished.timed_out && finished.exit.is_none());
        assert!(finished.elapsed < Duration::from_secs(2));
        assert!(started.elapsed() >= GRACE && started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn a_signal_ends_the_process_with_what_it_started_and_names_itself() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("survivor");
        let stop = Stop::default();
        let raiser = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(500));
                stop.raise(SIGTERM);
            })
        };
        let started = Instant::now();
        let error = working(job(
            &background(&marker),
            &[dir.path()],
            Duration::from_secs(60),
            &stop,
        ))
        .unwrap_err();
        raiser.join().unwrap();
        assert!(error.to_string().contains("SIGTERM"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_secs(3));
        assert!(!marker.exists(), "the background child went on");

        // A signal that came between two waits stops the next before it starts anything.
        let error = working(job(
            "touch started",
            &[dir.path()],
            Duration::from_secs(60),
            &stop,
        ))
        .unwrap_err();
        assert!(error.to_string().contains("SIGTERM"), "{error}");
        assert!(!dir.path().join("started").exists());
    }

    #[test]
    fn the_signals_of_the_task_are_caught_and_end_the_wait() {
        let dir = tempfile::tempdir().unwrap();
        let stop = Stop::catch().unwrap();
        let raiser = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            kill_process(getpid(), Signal::TERM).unwrap();
        });
        let error = working(job(
            "sleep 60",
            &[dir.path()],
            Duration::from_secs(60),
            &stop,
        ))
        .unwrap_err();
        raiser.join().unwrap();
        assert!(error.to_string().contains("SIGTERM"), "{error}");
    }

    #[test]
    fn a_sweep_leaves_the_processes_of_other_directories_alone() {
        let swept = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let mut bystander = Command::new("sleep")
            .arg("30")
            .current_dir(other.path())
            .spawn()
            .unwrap();
        let mut inside = Command::new("sleep")
            .arg("30")
            .current_dir(swept.path())
            .spawn()
            .unwrap();
        sweep(&[swept.path()]).unwrap();
        assert!(inside.wait().is_ok_and(|status| !status.success()));
        assert!(bystander.try_wait().unwrap().is_none(), "it was killed");
        bystander.kill().unwrap();
        bystander.wait().unwrap();
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A sweep refuses a directory the task did not make"
    )]
    #[test]
    fn a_sweep_refuses_a_directory_the_task_did_not_make() {
        let made = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let roots = with_canonical_names([made.path().to_path_buf()]);
        let run = made.path().join("run");
        fs::create_dir_all(&run).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), made.path().join("link")).unwrap();
        let home =
            std::env::var_os("HOME").map_or_else(|| PathBuf::from("/home/nobody"), Into::into);
        let beside = PathBuf::from(format!("{}x", made.path().display()));
        let escaping = run.join("../../..");

        // Only the checks are run here: a sweep of `/` or of the home directory that was not refused would end the processes of the machine.
        let refused = [
            Path::new(""),
            Path::new("/"),
            home.as_path(),
            Path::new("run"),
            Path::new("."),
            Path::new(".."),
            elsewhere.path(),
            // The directory the task made is not one to sweep as a whole, and a name that only starts like it is another directory.
            made.path(),
            beside.as_path(),
            // Behind a link, or behind `..`, the directory is another one.
            &made.path().join("link"),
            &escaping,
            &made.path().join("../x"),
        ];
        for dir in refused {
            let error = sweepable_dirs(&[dir], &roots).unwrap_err().to_string();
            assert!(
                error.contains(&format!("`{}`", dir.display())) && error.contains("this task made"),
                "{error}"
            );
        }

        // One directory the task did not make refuses the whole sweep, and the ones below the directories it made are taken in their canonical form.
        let canonical = fs::canonicalize(&run).unwrap();
        assert_eq!(sweepable_dirs(&[&run], &roots).unwrap(), [canonical]);
        let error = sweepable_dirs(&[&run, elsewhere.path()], &roots).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&elsewhere.path().display().to_string())
        );
        assert!(!sweepable(Path::new("/a/bc"), &[PathBuf::from("/a/b")]));
        assert!(sweepable(Path::new("/a/b/c"), &[PathBuf::from("/a/b")]));
        assert!(!sweepable(Path::new("/a/b"), &[PathBuf::from("/a/b")]));
        assert!(!sweepable(Path::new("/a/b/c"), &[]));

        // The roots of the task are the directories it makes for the checkouts and for the runs, and the home directory, `/`, and the directories of its binary and of `node` are none of them.
        let sweep_roots = sweep_roots();
        let real = with_canonical_names(checkout::made_by_the_task());
        for dir in &real {
            assert!(sweep_roots.contains(dir), "{}", dir.display());
        }
        assert!(!sweepable(&home, &real) && !sweepable(Path::new("/"), &real));
        assert!(!sweepable(&checkout::task_dir(), &real));
        assert!(!sweepable(&checkout::node_dir(), &real));
        let runs_checkout = checkout::directory(checkout::Purpose::Runs, "vite");
        assert!(sweepable(&runs_checkout, &real));
        assert!(sweepable(&checkout::tmp_root().join("r-abc"), &real));
    }

    #[test]
    fn a_refused_sweep_ends_nothing() {
        let made = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let roots = with_canonical_names([made.path().to_path_buf()]);
        let run = made.path().join("run");
        fs::create_dir_all(&run).unwrap();
        let mut inside = Command::new("sleep")
            .arg("30")
            .current_dir(&run)
            .spawn()
            .unwrap();
        let mut outside = Command::new("sleep")
            .arg("30")
            .current_dir(elsewhere.path())
            .spawn()
            .unwrap();
        // The directory that is the task's is first, and still nothing is ended.
        assert!(sweep_below(&[&run, elsewhere.path()], &roots).is_err());
        assert!(inside.try_wait().unwrap().is_none(), "it was killed");
        assert!(outside.try_wait().unwrap().is_none(), "it was killed");
        // Without the other directory the same sweep ends what works in `run` and leaves the rest.
        sweep_below(&[&run], &roots).unwrap();
        assert!(inside.wait().is_ok_and(|status| !status.success()));
        assert!(outside.try_wait().unwrap().is_none(), "it was killed");
        outside.kill().unwrap();
        outside.wait().unwrap();
    }

    #[test]
    fn this_process_and_its_parents_are_never_swept() {
        let keep = ancestry().unwrap();
        assert!(keep.contains(&std::process::id()));
        assert!(keep.contains(&getppid_raw()));
        assert!(
            working_directories()
                .unwrap()
                .contains(&(std::process::id(), std::env::current_dir().unwrap())),
            "this process is listed with the directory it stands in"
        );
    }

    fn getppid_raw() -> u32 {
        rustix::process::getppid().map_or(0, |pid| pid.as_raw_nonzero().get().unsigned_abs())
    }

    #[test]
    fn the_output_of_lsof_is_read_by_process() {
        let text = "p361\nfcwd\nn/\np478\nfcwd\nn/private/var/folders/a b/T/checkout\np9\nfcwd\n";
        assert_eq!(
            parse_lsof(text),
            [
                (361, PathBuf::from("/")),
                (478, PathBuf::from("/private/var/folders/a b/T/checkout"))
            ]
        );
    }

    /// The body of `has_exited` in `source`: from its signature to the closing brace that starts a line.
    fn has_exited_in(source: &str) -> Option<&str> {
        let signature = concat!("fn has_exited", "(child: &Child) -> bool {");
        let rest = source.get(source.find(signature)?..)?;
        rest.get(..rest.find("\n}\n")?)
    }

    #[test]
    fn has_exited_is_the_same_call_as_the_one_the_config_crate_has() {
        let ours = has_exited_in(include_str!("process.rs"));
        let theirs = has_exited_in(include_str!(
            "../../../crates/config/src/external/process.rs"
        ));
        assert!(ours.is_some(), "has_exited is not in this file");
        assert_eq!(
            ours, theirs,
            "the copy of has_exited has drifted from crates/config/src/external/process.rs"
        );
    }

    #[test]
    fn two_calls_to_catch_share_what_they_caught() {
        let first = Stop::catch().unwrap();
        let second = Stop::catch().unwrap();
        assert!(Arc::ptr_eq(&first.signal, &second.signal));
        assert!(Arc::ptr_eq(&first.arrived, &second.arrived));
        first.raise(SIGINT);
        assert!(second.check().is_err(), "one holds what the other caught");
        // The signal is process-wide, so it is not left for the next test of this process.
        first.signal.store(0, Ordering::Relaxed);
    }

    /// The test runs itself again as a child that catches SIGTERM, raises it twice and says what it saw: the first is caught and the second ends the process.
    #[gsttt_scenario::scenario("agent-benchmarks", "A second signal ends the task at once")]
    #[test]
    fn a_second_signal_ends_the_task_at_once() {
        use std::os::unix::process::ExitStatusExt as _;

        const CHILD: &str = "GSTTT_TEST_STOP_TWICE";
        if std::env::var_os(CHILD).is_some() {
            let stop = Stop::catch().unwrap();
            low_level::raise(SIGTERM).unwrap();
            assert!(stop.check().is_err());
            println!("the first signal was caught");
            low_level::raise(SIGTERM).unwrap();
            println!("the second signal was caught too");
            return;
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "agents::process::tests::a_second_signal_ends_the_task_at_once",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(said.contains("the first signal was caught"), "{said}");
        assert!(!said.contains("too"), "{said}");
        assert_eq!(output.status.signal(), Some(SIGTERM), "{said}");
    }
}
