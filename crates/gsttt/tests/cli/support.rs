//! What the tests that run the binary share: a sandbox for its environment, and what a run left behind.

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rustix::fs::{Mode, OFlags};
use rustix::pty::OpenptFlags;
use serde_json::Value;
use tempfile::TempDir;

/// The directories a run may touch, so that no test reads or writes the real home, cache or state of the user.
pub(crate) struct Sandbox {
    pub(crate) home: TempDir,
    pub(crate) cache: TempDir,
    pub(crate) state: TempDir,
    /// The directory the binary starts in. It is in no git repository.
    pub(crate) work: TempDir,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        let dir = || tempfile::tempdir().unwrap();
        Self {
            home: dir(),
            cache: dir(),
            state: dir(),
            work: dir(),
        }
    }

    /// The binary, started in the work directory with an environment of its own: only the sandbox's `HOME`, `XDG_CACHE_HOME` and `XDG_STATE_HOME`.
    pub(crate) fn gsttt(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gsttt"));
        command
            .env_clear()
            .env("HOME", self.home.path())
            .env("XDG_CACHE_HOME", self.cache.path())
            .env("XDG_STATE_HOME", self.state.path())
            .current_dir(self.work.path());
        command
    }

    /// Runs the binary with `args` in the work directory.
    pub(crate) fn run(&self, args: &[&str]) -> Ran {
        ran(self.gsttt().args(args))
    }

    /// Runs the binary with `args` in `dir`.
    pub(crate) fn run_in(&self, dir: &Path, args: &[&str]) -> Ran {
        ran(self.gsttt().current_dir(dir).args(args))
    }

    /// Runs the binary with `args` in `dir`, with `input` on its stdin, which is a pipe.
    pub(crate) fn run_piped(&self, dir: &Path, args: &[&str], input: &str) -> Ran {
        let mut child = self
            .gsttt()
            .current_dir(dir)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // A run that does not read its stdin may end first, and the pipe is then closed.
        child.stdin.take().unwrap().write_all(input.as_bytes()).ok();
        let output = child.wait_with_output().unwrap();
        Ran::of(output)
    }

    /// Runs the binary with `args` in `dir`, with stdin and stderr on one pseudo-terminal where `typed` was typed before the run began to read. `stderr` of the result is what the terminal showed, the echo of `typed` included, with its line ends read as `\n`.
    pub(crate) fn run_typing(&self, dir: &Path, args: &[&str], typed: &str) -> Ran {
        let master = rustix::pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        rustix::pty::grantpt(&master).unwrap();
        rustix::pty::unlockpt(&master).unwrap();
        let name = rustix::pty::ptsname(&master, Vec::new()).unwrap();
        let slave = rustix::fs::open(
            name.as_c_str(),
            OFlags::RDWR | OFlags::NOCTTY,
            Mode::empty(),
        )
        .unwrap();
        let mut keyboard = File::from(master.try_clone().unwrap());
        let mut screen = File::from(master);
        keyboard.write_all(typed.as_bytes()).unwrap();
        // The screen is read while the run writes to it, and not afterwards: some systems drop what a terminal holds when its last user closes it.
        let seen = std::thread::spawn(move || {
            let mut shown = Vec::new();
            // The read ends with an error on some systems when the run has closed the terminal.
            screen.read_to_end(&mut shown).ok();
            shown
        });
        let mut command = self.gsttt();
        command
            .current_dir(dir)
            .args(args)
            .stdin(slave.try_clone().unwrap())
            .stderr(slave);
        let output = command.output().unwrap();
        // The terminal ends, and the screen with it, when nothing here holds its slave side any more.
        drop(command);
        let shown = seen.join().unwrap();
        drop(keyboard);
        let mut run = Ran::of(output);
        run.stderr = String::from_utf8(shown).unwrap().replace("\r\n", "\n");
        run
    }

    /// A git repository `name` in the work directory whose config declares the external connector `fake`: the command `/bin/sh fake.sh` over the files `*.fake`. The script creates [`Fake::marker`] and ends, so a run that started the connector leaves the marker behind. Every repository made by one sandbox has the same script, and so the same marker.
    pub(crate) fn fake_external(&self, name: &str) -> Fake {
        let root = self.work.path().join(name);
        std::fs::create_dir_all(&root).unwrap();
        init_repo(&root);
        typescript_project(&root);
        let marker = self.work.path().join("started");
        write(&root, "thing.fake", "a file that `fake` claims\n");
        write(&root, "fake.sh", &format!(": > '{}'\n", marker.display()));
        write(&root, FAKE_CONFIG_PATH, FAKE_CONFIG);
        commit_all(&root);
        Fake {
            root: root.canonicalize().unwrap(),
            marker,
        }
    }

    /// A git repository `repo` in the work directory, holding the project of [`typescript_project`] in a first commit. Returns the canonical path of its root.
    pub(crate) fn repository(&self) -> PathBuf {
        let repo = self.work.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        typescript_project(&repo);
        commit_all(&repo);
        repo.canonicalize().unwrap()
    }
}

/// Where the config of a repository made by [`Sandbox::fake_external`] is.
pub(crate) const FAKE_CONFIG_PATH: &str = ".gsttt/config.toml";

/// The config of a repository made by [`Sandbox::fake_external`].
pub(crate) const FAKE_CONFIG: &str =
    "[[external]]\nname = \"fake\"\ncmd = [\"/bin/sh\", \"fake.sh\"]\nfiles = [\"**/*.fake\"]\n";

/// A repository with an external connector whose program only marks that it started.
#[derive(Debug)]
pub(crate) struct Fake {
    /// The canonical path of the root.
    pub(crate) root: PathBuf,
    /// The file the program creates. It is outside the repository, and a run that did not start the connector leaves it out.
    pub(crate) marker: PathBuf,
}

/// What a run printed and how it ended.
#[derive(Debug)]
pub(crate) struct Ran {
    /// The exit code, `None` when a signal ended the process.
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl Ran {
    fn of(output: std::process::Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).unwrap(),
            stderr: String::from_utf8(output.stderr).unwrap(),
        }
    }
}

/// The JSON document a run that succeeded printed alone on stdout.
pub(crate) fn json(run: &Ran) -> Value {
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stderr, "", "{run:?}");
    serde_json::from_str(&run.stdout).unwrap_or_else(|error| panic!("{error}: {run:?}"))
}

/// Runs `command` to its end with its stdin closed.
pub(crate) fn ran(command: &mut Command) -> Ran {
    Ran::of(command.output().unwrap())
}

/// Whether anything was written under `dir`.
pub(crate) fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).unwrap().next().is_none()
}

/// Writes `text` to `path` under `dir`, creating the directories above it.
pub(crate) fn write(dir: impl AsRef<Path>, path: &str, text: &str) {
    let file = dir.as_ref().join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// Like [`write`], with the permissions of a program: `rwxr-xr-x`.
pub(crate) fn write_executable(dir: impl AsRef<Path>, path: &str, text: &str) {
    write(&dir, path, text);
    std::fs::set_permissions(
        dir.as_ref().join(path),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

/// Writes three TypeScript files under `dir`: `app/a.ts` imports `lib/util.ts`, and `app/b.ts` imports nothing.
pub(crate) fn typescript_project(dir: impl AsRef<Path>) {
    let dir = dir.as_ref();
    write(dir, "lib/util.ts", "export const util = 1;\n");
    write(
        dir,
        "app/a.ts",
        "import { util } from '../lib/util';\nexport const a = util;\n",
    );
    write(dir, "app/b.ts", "export const b = 2;\n");
}

/// Every file and directory under `dir`, as paths from `dir`, in order.
pub(crate) fn listing(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, from: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            found.push(path.strip_prefix(from).unwrap().display().to_string());
            if path.is_dir() {
                walk(&path, from, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, dir, &mut found);
    found.sort();
    found
}

/// Runs `git` with `args` in `dir`, whatever git settings the machine has, and returns what it printed.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Makes `dir` the work tree of a new git repository.
pub(crate) fn init_repo(dir: &Path) {
    git(dir, &["init", "--quiet"]);
}

/// Commits everything under `dir`, a git repository, and returns the id of the commit.
pub(crate) fn commit_all(dir: &Path) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", "first"]);
    git(dir, &["rev-parse", "HEAD"]).trim().to_owned()
}
