//! What the tests that run the binary share: a sandbox for its environment, and what a run left behind.

use std::process::Command;

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
}

/// What a run printed and how it ended.
#[derive(Debug)]
pub(crate) struct Ran {
    /// The exit code, `None` when a signal ended the process.
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
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
