//! A task works on the whole repository, from whichever of its directories it is run in.

use std::path::{Path, PathBuf};

use crate::support::{TARGET, built, commit, printed, repository, write, xtask, xtask_command};

/// A directory deep in the repository in `root`, like the one a developer is in when they run a task.
fn subdirectory(root: &Path) -> PathBuf {
    let directory = root.join("crates").join("core");
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn the_version_is_counted_from_a_subdirectory() {
    let repo = repository("0.1.0");
    commit(repo.path(), "feat: second");

    let output = xtask(&subdirectory(repo.path()), &["version"]);

    assert!(output.status.success(), "{}", printed(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0.1.2\n");
}

#[test]
fn the_notes_are_listed_from_a_subdirectory() {
    let repo = repository("0.1.0");
    commit(repo.path(), "feat: second");

    let output = xtask(&subdirectory(repo.path()), &["notes"]);

    assert!(output.status.success(), "{}", printed(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "- feat: second\n- chore: start\n"
    );
}

#[test]
fn the_check_reads_the_workflows_of_the_repository_from_a_subdirectory() {
    let repo = repository("0.1.0");
    write(
        repo.path(),
        ".github/workflows/ci.yml",
        "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v4\n",
    );

    let output = xtask(&subdirectory(repo.path()), &["check"]);

    assert!(!output.status.success(), "{}", printed(&output));
    assert!(
        printed(&output).contains(".github/workflows/ci.yml:4:"),
        "{}",
        printed(&output)
    );
}

#[test]
fn a_package_is_made_from_a_subdirectory() {
    let repo = repository("0.1.0");
    let target_dir = built(b"binary bytes");

    let output = xtask_command(&subdirectory(repo.path()))
        .env("CARGO_TARGET_DIR", target_dir.path())
        .args(["package", "--target", TARGET])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(
        target_dir
            .path()
            .join("packages")
            .join(format!("gsttt-0.1.1-{TARGET}.tar.gz"))
            .is_file()
    );
}

#[test]
fn a_relative_target_directory_is_read_from_the_directory_the_task_runs_in() {
    let repo = repository("0.1.0");
    let here = subdirectory(repo.path());
    // Cargo reads a relative `CARGO_TARGET_DIR` from the directory it was started in, and so must the task.
    write(&here, &format!("out/{TARGET}/dist/gsttt"), "binary bytes");

    let output = xtask_command(&here)
        .env("CARGO_TARGET_DIR", "out")
        .args(["package", "--target", TARGET])
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", printed(&output));
    assert!(
        here.join("out")
            .join("packages")
            .join(format!("gsttt-0.1.1-{TARGET}.tar.gz"))
            .is_file()
    );
}

#[test]
fn a_directory_outside_a_repository_is_named_in_the_error() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().parent().unwrap();

    let output = xtask_command(directory.path())
        // Stops git looking for a repository above the temporary directory.
        .env("GIT_CEILING_DIRECTORIES", parent)
        .arg("version")
        .output()
        .unwrap();

    assert!(!output.status.success(), "{}", printed(&output));
    let message = printed(&output);
    assert!(
        message.contains("is not inside a git repository"),
        "{message}"
    );
    assert!(
        message.contains(&directory.path().display().to_string()),
        "{message}"
    );
}
