//! `cargo xtask notes`.

use crate::support::{commit, git, printed, repository, xtask};

fn notes(directory: &std::path::Path) -> String {
    let output = xtask(directory, &["notes"]);
    assert!(output.status.success(), "{}", printed(&output));
    String::from_utf8(output.stdout).unwrap()
}

#[gsttt_scenario::scenario("distribution", "Notes list the commits since the last release")]
#[test]
fn notes_list_the_commits_since_the_last_release() {
    let repo = repository("0.1.0");
    commit(repo.path(), "feat: before the release");
    git(repo.path(), &["tag", "v0.1.3"]);
    for subject in ["fix: first", "feat: second", "docs: third"] {
        commit(repo.path(), subject);
    }

    assert_eq!(
        notes(repo.path()),
        "- docs: third\n- feat: second\n- fix: first\n"
    );
}

#[test]
fn notes_start_after_the_newest_version_not_the_newest_name() {
    let repo = repository("0.1.0");
    git(repo.path(), &["tag", "v0.1.9"]);
    commit(repo.path(), "feat: between");
    git(repo.path(), &["tag", "v0.1.10"]);
    commit(repo.path(), "fix: after");

    assert_eq!(notes(repo.path()), "- fix: after\n");
}

#[test]
fn notes_without_a_release_list_every_commit() {
    let repo = repository("0.1.0");
    commit(repo.path(), "feat: second");

    assert_eq!(notes(repo.path()), "- feat: second\n- chore: start\n");
}
