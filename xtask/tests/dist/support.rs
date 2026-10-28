//! Helpers shared by the tests: repositories made in a temporary directory and the `xtask` binary run on them.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// The targets a release is built for.
pub(crate) const RELEASE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

/// The target the tests build a package for.
pub(crate) const TARGET: &str = RELEASE_TARGETS[0];

/// The root of the workspace this crate belongs to.
pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Makes `command` ignore the git configuration of the machine and commit as a fixed author.
pub(crate) fn isolate_git(command: &mut Command) -> &mut Command {
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

/// Runs `xtask package --target <target>` in `repo`, with the release build of `target` in `target_dir`.
pub(crate) fn package(repo: &Path, target_dir: &Path, target: &str) -> Output {
    xtask_command(repo)
        .env("CARGO_TARGET_DIR", target_dir)
        .args(["package", "--target", target])
        .output()
        .unwrap()
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

/// A target directory holding a release build of `TARGET`, as `cargo build --profile dist` leaves it.
pub(crate) fn built(binary: &[u8]) -> TempDir {
    let target_dir = tempfile::tempdir().unwrap();
    let directory = target_dir.path().join(TARGET).join("dist");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("gsttt"), binary).unwrap();
    target_dir
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

/// Like [`write`], with the permissions of a program: `rwxr-xr-x`.
pub(crate) fn write_executable(root: &Path, relative: &str, content: &str) {
    write(root, relative, content);
    std::fs::set_permissions(root.join(relative), std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Sets the permission bits of `relative` under `root` to `mode`.
pub(crate) fn set_mode(root: &Path, relative: &str, mode: u32) {
    std::fs::set_permissions(root.join(relative), std::fs::Permissions::from_mode(mode)).unwrap();
}

/// Everything `output` printed: standard output, then standard error.
pub(crate) fn printed(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
