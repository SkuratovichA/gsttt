//! `cargo xtask version`.

use crate::support::{commit, git, printed, repository, xtask};

#[gsttt_scenario::scenario("distribution", "The version counts commits")]
#[test]
fn the_version_counts_commits() {
    let repo = repository("0.1.0");
    for number in 2..=42 {
        commit(repo.path(), &format!("chore: commit {number}"));
    }

    let output = xtask(repo.path(), &["version"]);

    assert!(output.status.success(), "{}", printed(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0.1.42\n");
}

#[test]
fn a_shallow_clone_has_no_version() {
    let origin = repository("0.1.0");
    commit(origin.path(), "chore: second");
    let clone = tempfile::tempdir().unwrap();
    let url = format!("file://{}", origin.path().display());
    git(
        clone.path(),
        &["clone", "--quiet", "--depth=1", &url, "shallow"],
    );

    let output = xtask(&clone.path().join("shallow"), &["version"]);

    assert!(!output.status.success());
    assert!(
        printed(&output).contains("shallow clone"),
        "{}",
        printed(&output)
    );
}
