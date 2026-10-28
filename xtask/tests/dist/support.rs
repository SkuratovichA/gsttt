//! Helpers shared by the tests: repositories made in a temporary directory and the `xtask` binary run on them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// The root of the workspace this crate belongs to.
pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Makes `command` ignore the git configuration of the machine and commit as a fixed author.
fn isolate_git(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

/// `xtask`, ready to run in `directory` as it would be from a checkout: the target directory of the test run is not inherited.
pub(crate) fn xtask_command(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    isolate_git(&mut command)
        .current_dir(directory)
        .env_remove("CARGO_TARGET_DIR");
    command
}

/// Runs `xtask <args>` in `directory`.
pub(crate) fn xtask(directory: &Path, args: &[&str]) -> Output {
    xtask_command(directory).args(args).output().unwrap()
}

/// Runs `git <args>` in `directory` and returns what it printed, without the trailing newline.
pub(crate) fn git(directory: &Path, args: &[&str]) -> String {
    let output = isolate_git(&mut Command::new("git"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}

/// A repository with one commit, whose `Cargo.toml` sets the workspace version to `version`, and a `LICENSE` and `README.md`.
pub(crate) fn repository(version: &str) -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    git(path, &["init", "--quiet", "--initial-branch=main"]);
    write(
        path,
        "Cargo.toml",
        &format!("[workspace.package]\nversion = \"{version}\"\n"),
    );
    write(path, "LICENSE", "license text\n");
    write(path, "README.md", "readme text\n");
    git(path, &["add", "."]);
    git(path, &["commit", "--quiet", "--message", "chore: start"]);
    directory
}

/// Adds an empty commit with the subject `subject`.
pub(crate) fn commit(directory: &Path, subject: &str) {
    git(
        directory,
        &["commit", "--quiet", "--allow-empty", "--message", subject],
    );
}

/// Writes `content` to `relative` under `root`, making the directories on the way.
pub(crate) fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// Everything `output` printed: standard output, then standard error.
pub(crate) fn printed(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
