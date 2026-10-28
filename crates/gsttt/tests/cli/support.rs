//! What the tests that run the binary share: a sandbox for its environment, and what a run left behind.

use std::path::{Path, PathBuf};
use std::process::Command;

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

    /// The binary, started in the work directory with an environment of its own: only the sandbox's `HOME`,
    /// `XDG_CACHE_HOME` and `XDG_STATE_HOME`.
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

    /// A git repository `repo` in the work directory, holding the project of [`typescript_project`] in a first
    /// commit. Returns the canonical path of its root.
    pub(crate) fn repository(&self) -> PathBuf {
        let repo = self.work.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_repo(&repo);
        typescript_project(&repo);
        commit_all(&repo);
        repo.canonicalize().unwrap()
    }
}

/// What a run printed and how it ended.
#[derive(Debug)]
pub(crate) struct Ran {
    /// The exit code, `None` when a signal ended the process.
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// The JSON document a run that succeeded printed alone on stdout.
pub(crate) fn json(run: &Ran) -> Value {
    assert_eq!(run.code, Some(0), "{run:?}");
    assert_eq!(run.stderr, "", "{run:?}");
    serde_json::from_str(&run.stdout).unwrap_or_else(|error| panic!("{error}: {run:?}"))
}

/// Runs `command` to its end with its stdin closed.
pub(crate) fn ran(command: &mut Command) -> Ran {
    let output = command.output().unwrap();
    Ran {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
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
