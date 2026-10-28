//! `gsttt` on this repository: the tree as it is committed is a corpus the parsers have to read, and a Rust file they fail on is a gap to fix.

use std::path::{Path, PathBuf};
use std::process::Command;

use gsttt_scenario::scenario;
use serde_json::Value;

use super::support::{Sandbox, commit_all, git, init_repo, json};

/// The root of this repository: the manifest directory of `crates/gsttt` is two levels below it.
fn root_of_this_repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

/// A copy of the committed tree of this repository in the work directory of `sandbox`, as a repository of one commit, the way a checkout of the tree is. Returns the canonical path of its root.
fn copy_of_this_repository(sandbox: &Sandbox) -> PathBuf {
    let tar = sandbox.work.path().join("tree.tar");
    let tar_argument = tar.to_str().unwrap();
    git(
        &root_of_this_repository(),
        &["archive", "--format=tar", "-o", tar_argument, "HEAD"],
    );
    let copy = sandbox.work.path().join("copy");
    std::fs::create_dir_all(&copy).unwrap();
    let unpacked = Command::new("tar")
        .arg("-xf")
        .arg(&tar)
        .arg("-C")
        .arg(&copy)
        .status()
        .unwrap();
    assert!(unpacked.success(), "tar could not unpack {tar:?}");
    init_repo(&copy);
    commit_all(&copy);
    copy.canonicalize().unwrap()
}

/// The Rust files of the coverage document `coverage` that were not read, each with how: the files a connector failed on and the files left out before any connector saw them. The files under `tests/fixtures/` are not counted, because some of them are broken on purpose.
fn unread_rust_files(coverage: &Value) -> Vec<String> {
    let failed = coverage["connectors"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|connector| connector["failed"].as_array().unwrap());
    failed
        .chain(coverage["skipped"].as_array().unwrap())
        .filter(|file| {
            let path = file["path"].as_str().unwrap();
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "rs")
                && !path.contains("tests/fixtures/")
        })
        .map(Value::to_string)
        .collect()
}

#[scenario("cli", "The repository indexes without a failed Rust file")]
#[test]
fn the_repository_indexes_without_a_failed_rust_file() {
    let sandbox = Sandbox::new();
    let copy = copy_of_this_repository(&sandbox);

    let indexed = sandbox.run_in(&copy, &["index"]);
    assert_eq!(indexed.code, Some(0), "{indexed:?}");

    let coverage = json(&sandbox.run_in(&copy, &["--json", "--no-refresh", "coverage"]));
    let unread = unread_rust_files(&coverage);
    assert!(
        unread.is_empty(),
        "these Rust files were not read:\n{}",
        unread.join("\n")
    );
}

/// The test above passes whenever no Rust file fails, so this one shows that the filter finds a Rust file that did, wherever the coverage lists it, and only that.
#[test]
fn a_rust_file_outside_the_fixtures_that_was_not_read_is_found() {
    let coverage = serde_json::json!({
        "connectors": [
            {"connector": "rust", "failed": [
                {"path": "crates/a/src/lib.rs", "failure": "syntax", "line": 3, "message": "expected an item"},
                {"path": "crates/a/tests/fixtures/broken/src/lib.rs", "failure": "syntax", "line": 1, "message": "expected an item"},
            ]},
            {"connector": "ts", "failed": [
                {"path": "web/app.ts", "failure": "not-utf8"},
            ]},
        ],
        "skipped": [
            {"path": "crates/b/src/generated.rs", "failure": "too-large", "bytes": 9, "limit": 1},
            {"path": "crates/b/notes.md", "failure": "not-utf8"},
        ],
    });

    let found = unread_rust_files(&coverage);

    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("crates/a/src/lib.rs"), "{found:?}");
    assert!(found[0].contains("expected an item"), "{found:?}");
    assert!(found[1].contains("crates/b/src/generated.rs"), "{found:?}");
}
